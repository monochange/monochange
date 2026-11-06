import 'package:flutter_test/flutter_test.dart';
import 'package:feedback_example/main.dart';

void main() {
  testWidgets('opens native feedback without requesting a token until needed', (
    tester,
  ) async {
    await tester.pumpWidget(const ExampleApp());
    await tester.tap(find.text('Share feedback'));
    await tester.pumpAndSettle();
    expect(find.text('Find similar requests'), findsOneWidget);
  });
}
