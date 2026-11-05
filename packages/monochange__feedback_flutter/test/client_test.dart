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
          expect(request.url.path, '/api/v1/feedback/1/suggestions');
          expect(jsonDecode(request.body), {'text': 'CSV export'});
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
}
