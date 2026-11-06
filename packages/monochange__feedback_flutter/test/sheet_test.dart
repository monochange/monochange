import 'dart:convert';
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:http/http.dart' as http;
import 'package:http/testing.dart';
import 'package:monochange_feedback/monochange_feedback.dart';

void main() {
  testWidgets('native feedback gathers a draft before offering submission', (
    tester,
  ) async {
    final client = FeedbackClient(
      endpoint: Uri.parse('https://feedback.example'),
      project: 1,
      getClientToken: () async => ClientToken(
        'session',
        DateTime.now().add(const Duration(minutes: 10)),
      ),
      client: MockClient((request) async {
        expect(request.url.path, '/api/v1/feedback/1/conversations');
        expect(jsonDecode(request.body)['message'], 'CSV export');
        return http.Response(
          jsonEncode({
            'conversation': {
              'id': 'draft',
              'version': 1,
              'kind': 'feature_request',
              'messages': [
                {'role': 'user', 'body': 'CSV export'},
                {'role': 'assistant', 'body': 'Which columns?'},
              ],
            },
            'similar': [],
            'mode': 'basic',
          }),
          200,
        );
      }),
    );
    await tester.pumpWidget(
      MaterialApp(
        home: Scaffold(
          body: FeedbackSheet(client: client, identityKey: 'user-1'),
        ),
      ),
    );
    expect(find.text('Submit a new request'), findsNothing);
    await tester.enterText(find.byType(TextField), 'CSV export');
    await tester.runAsync(() async {
      await tester.tap(find.text('Find similar requests'));
      await Future<void>.delayed(Duration.zero);
    });
    await tester.pumpAndSettle();
    expect(find.text('assistant: Which columns?'), findsOneWidget);
    expect(find.text('Submit a new request'), findsOneWidget);
    await tester.pumpWidget(const SizedBox());
    client.close();
  });
}
