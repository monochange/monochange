/// Client entrypoints built on top of `package:acme_models`.
library;

import 'package:acme_models/acme_models.dart';

/// Loads and validates documents for a named Acme environment.
class Client {
  /// Creates a client that labels documents with [name].
  Client(this.name);

  /// The label applied to every document this client loads.
  final String name;

  /// Parses [source] and returns a status line.
  String load(String source) {
    final document = Document(source);
    final problems = document.validate();
    if (problems.isNotEmpty) {
      throw FormatException('$name: ${problems.first}');
    }
    return '$name: ${document.lineCount} lines';
  }
}
