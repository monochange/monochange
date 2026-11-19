import 'dart:async';
import 'dart:convert';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:monochange_feedback/monochange_feedback.dart';

void main() {
  test(
    'refreshes expired credentials once and keeps user identity off the wire',
    () async {
      var issued = 0;
      var calls = 0;
      final client = FeedbackClient(
        endpoint: Uri.parse('https://feedback.example'),
        project: 1,
        getClientToken: () async => ClientToken(
          'session-${++issued}',
          DateTime.now().add(const Duration(minutes: 10)),
        ),
        client: MockClient((request) async {
          expect(request.url.path, '/api/feedback_sdk_suggestions');
          expect(request.url.queryParameters, {'project': '1'});
          expect(jsonDecode(request.body), {
            'project': 1,
            'input': {'text': 'CSV export'},
          });
          calls++;
          return calls == 1
              ? http.Response(
                  '{"code":"unauthorized","message":"expired"}',
                  401,
                )
              : http.Response(
                  '{"reply":"Review matches","similar":[],"mode":"basic"}',
                  200,
                );
        }),
      );
      expect((await client.suggest('CSV export'))['reply'], 'Review matches');
      expect(issued, 2);
      expect(calls, 2);
      client.close();
    },
  );

  test('submission retries reuse the same key', () async {
    final keys = <String>[];
    final client = FeedbackClient(
      endpoint: Uri.parse('https://feedback.example'),
      project: 1,
      getClientToken: () async => ClientToken(
        'session',
        DateTime.now().add(const Duration(minutes: 10)),
      ),
      client: MockClient((request) async {
        keys.add(request.headers['Idempotency-Key']!);
        if (keys.length == 1) throw http.ClientException('connection lost');
        return http.Response('{"id":"fb-1","stage":"discussing"}', 200);
      }),
    );
    await expectLater(
      client.submit('draft', 1),
      throwsA(isA<http.ClientException>()),
    );
    expect((await client.submit('draft', 1))['id'], 'fb-1');
    expect(keys[0], keys[1]);
    client.close();
  });

  test('an account change invalidates a pending token', () async {
    final token = Completer<ClientToken>();
    var calls = 0;
    final client = FeedbackClient(
      endpoint: Uri.parse('https://feedback.example'),
      project: 1,
      getClientToken: () => token.future,
      client: MockClient((_) async {
        calls++;
        return http.Response('{}', 200);
      }),
    );
    final pending = client.suggest('CSV export');
    client.reset();
    token.complete(
      ClientToken('old-user', DateTime.now().add(const Duration(minutes: 10))),
    );
    await expectLater(pending, throwsA(isA<FeedbackException>()));
    expect(calls, 0);
    client.close();
  });

  FeedbackClient recording(
    List<http.Request> sent,
    http.Response Function(http.Request request) answer,
  ) => FeedbackClient(
    endpoint: Uri.parse('https://feedback.example/'),
    project: 7,
    getClientToken: () async =>
        ClientToken('session', DateTime.now().add(const Duration(minutes: 10))),
    getContext: () => {'route': '/settings?tab=billing#export'},
    client: MockClient((request) async {
      sent.add(request);
      return answer(request);
    }),
  );

  Future<void> everyCall(FeedbackClient client) async {
    final conversation = {'id': 'draft/1', 'version': 2};
    await client.suggest('CSV');
    await client.startConversation('Export CSV');
    await client.getConversation('draft/1');
    await client.sendMessage(conversation, 'Monthly');
    await client.submit('draft/1', 2);
    await client.getItem('fb-1');
    await client.reply('fb-1', 'Thanks');
    await client.vote('fb-1');
    await client.vote('fb-1', on: false);
  }

  test('calls each RPC operation with the project', () async {
    final sent = <http.Request>[];
    final client = recording(sent, (_) => http.Response('{}', 200));
    await everyCall(client);
    expect(sent.map((request) => request.url.toString()), [
      for (final operation in [
        'suggestions',
        'create_conversation',
        'conversation',
        'conversation_message',
        'submit',
        'item',
        'reply',
        'vote',
        'vote',
      ])
        'https://feedback.example/api/feedback_sdk_$operation?project=7',
    ]);
    expect(sent.every((request) => request.method == 'POST'), isTrue);
    expect(sent.map((request) => jsonDecode(request.body)), [
      {
        'project': 7,
        'input': {'text': 'CSV'},
      },
      {
        'project': 7,
        'input': {
          'message': 'Export CSV',
          'kind': 'feature_request',
          'context': {'route': '/settings'},
        },
      },
      {'project': 7, 'id': 'draft/1'},
      {
        'project': 7,
        'id': 'draft/1',
        'input': {'body': 'Monthly', 'version': 2},
      },
      {
        'project': 7,
        'input': {'conversation': 'draft/1', 'version': 2},
      },
      {'project': 7, 'id': 'fb-1'},
      {
        'project': 7,
        'id': 'fb-1',
        'input': {'body': 'Thanks'},
      },
      {'project': 7, 'id': 'fb-1', 'voted': true},
      {'project': 7, 'id': 'fb-1', 'voted': false},
    ]);
    expect(sent[4].headers['Idempotency-Key'], isNotNull);
    client.close();
  });

  test('keeps an RPC failure that carries a code', () async {
    final sent = <http.Request>[];
    final client = recording(
      sent,
      (_) => http.Response(
        '{"status":404,"code":"not_found","message":"No such request"}',
        404,
      ),
    );
    await expectLater(
      client.getItem('fb-9'),
      throwsA(
        isA<FeedbackException>()
            .having((error) => error.code, 'code', 'not_found')
            .having((error) => error.status, 'status', 404),
      ),
    );
    expect(sent, hasLength(1));
    client.close();
  });

  test('falls back to the REST routes on an older site', () async {
    final sent = <http.Request>[];
    final client = recording(
      sent,
      (request) => request.url.path.startsWith('/api/feedback_sdk_')
          ? http.Response('<html>Not found</html>', 404)
          : http.Response('{}', 200),
    );
    await everyCall(client);
    const base = 'https://feedback.example/api/v1/feedback/7';
    expect(sent.skip(1).map((request) => '${request.method} ${request.url}'), [
      'POST $base/suggestions',
      'POST $base/conversations',
      'GET $base/conversations/draft%2F1',
      'POST $base/conversations/draft%2F1/messages',
      'POST $base/items',
      'GET $base/items/fb-1',
      'POST $base/items/fb-1/messages',
      'PUT $base/items/fb-1/vote',
      'DELETE $base/items/fb-1/vote',
    ]);
    expect(jsonDecode(sent[1].body), {'text': 'CSV'});
    expect(sent[5].headers['Idempotency-Key'], isNotNull);
    client.close();
  });

  test('falls back when an older site answers a JSON 404', () async {
    final sent = <http.Request>[];
    final client = recording(
      sent,
      (request) => request.url.path.startsWith('/api/feedback_sdk_')
          ? http.Response('{"error":"missing"}', 404)
          : http.Response('{"reply":"REST"}', 200),
    );
    expect((await client.suggest('CSV'))['reply'], 'REST');
    expect(sent[1].url.path, '/api/v1/feedback/7/suggestions');
    client.close();
  });
}
