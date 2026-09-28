/// Domain models shared by the Acme client applications.
library;

/// A parsed configuration document.
class Document {
  /// Creates a document from raw [source] text.
  Document(this.source);

  /// The raw text this document was parsed from.
  final String source;

  /// The number of non-empty lines in the document.
  int get lineCount =>
      source.split('\n').where((line) => line.trim().isNotEmpty).length;

  /// Returns every problem found in the document.
  List<String> validate() {
    if (source.trim().isEmpty) {
      return const ['document is empty'];
    }
    return const [];
  }
}
