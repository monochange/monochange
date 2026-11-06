import 'dart:convert';
import 'package:flutter/material.dart';
import 'package:http/http.dart' as http;
import 'package:monochange_feedback/monochange_feedback.dart';

void main() => runApp(const ExampleApp());

class ExampleApp extends StatefulWidget {
  const ExampleApp({super.key});
  @override
  State<ExampleApp> createState() => _ExampleAppState();
}

class _ExampleAppState extends State<ExampleApp> {
  late final FeedbackClient feedback;
  @override
  void initState() {
    super.initState();
    feedback = FeedbackClient(
      endpoint: Uri.parse(
        const String.fromEnvironment(
          'FEEDBACK_ENDPOINT',
          defaultValue: 'https://monochange.dev',
        ),
      ),
      project: const int.fromEnvironment('FEEDBACK_PROJECT', defaultValue: 1),
      getContext: () => {'route': '/example', 'app_version': '1.0.0'},
      getClientToken: () async {
        // Use your app's authenticated HTTP client here. Never embed the backend credential.
        const tokenUrl = String.fromEnvironment('FEEDBACK_TOKEN_URL');
        if (tokenUrl.isEmpty) {
          throw const FeedbackException(
            'Configure your authenticated host token endpoint',
          );
        }

        final response = await http.get(Uri.parse(tokenUrl));
        if (response.statusCode != 200) {
          throw const FeedbackException('Sign in to the host app first');
        }
        return ClientToken.fromJson(jsonDecode(response.body) as Json);
      },
    );
  }

  @override
  void dispose() {
    feedback.close();
    super.dispose();
  }

  @override
  Widget build(BuildContext context) => MaterialApp(
    title: 'Feedback example',
    theme: ThemeData(colorSchemeSeed: const Color(0xff4f46e5)),
    home: Scaffold(
      appBar: AppBar(title: const Text('Feedback example')),
      body: Builder(
        builder: (context) => Center(
          child: FilledButton(
            onPressed: () => showModalBottomSheet<void>(
              context: context,
              isScrollControlled: true,
              useSafeArea: true,
              builder: (context) => FeedbackSheet(
                client: feedback,
                identityKey: 'example-user',
                onClose: () => Navigator.pop(context),
              ),
            ),
            child: const Text('Share feedback'),
          ),
        ),
      ),
    ),
  );
}
