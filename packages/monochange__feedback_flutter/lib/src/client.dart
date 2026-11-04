import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';
import 'package:http/http.dart' as http;

typedef Json = Map<String, dynamic>;

class ClientToken {
  final String token;
  final DateTime expiresAt;
  const ClientToken(this.token, this.expiresAt);

  factory ClientToken.fromJson(Json value) => ClientToken(
    value['token'] as String,
    DateTime.fromMillisecondsSinceEpoch(
      (value['expires_at'] as num).toInt() * 1000,
    ),
  );
}

class FeedbackException implements Exception {
  final String message;
  final String code;
  final int status;
  final int? retryAfter;
  const FeedbackException(
    this.message, {
    this.code = 'request_failed',
    this.status = 0,
    this.retryAfter,
  });
  @override
  String toString() => message;
}

/// Host authentication stays in the app. This client never stores a backend key.
class FeedbackClient {
  final Uri base;
  final Future<ClientToken> Function() getClientToken;
  final Json? Function()? getContext;
  final bool _ownsHttp;
  http.Client _http;
  ClientToken? _token;
  Future<ClientToken>? _refreshing;
  int _generation = 0;
  bool _disposed = false;
  final Map<String, String> _submissionKeys = {};
  static int _sequence = 0;

  FeedbackClient({
    required Uri endpoint,
    required int project,
    required this.getClientToken,
    this.getContext,
    http.Client? client,
  }) : base = Uri.parse(
         '${endpoint.toString().replaceFirst(RegExp(r'/$'), '')}/api/v1/feedback/$project/',
       ),
       _http = client ?? http.Client(),
       _ownsHttp = client == null {
    if (project < 1 ||
        endpoint.hasQuery ||
        endpoint.hasFragment ||
        endpoint.userInfo.isNotEmpty ||
        !(endpoint.scheme == 'https' ||
            endpoint.scheme == 'http' &&
                ['localhost', '127.0.0.1', '::1'].contains(endpoint.host))) {
      throw const FeedbackException(
        'Invalid feedback endpoint or project',
        code: 'invalid_configuration',
      );
    }
  }

  Future<ClientToken> _session() async {
    if (_disposed) {
      throw const FeedbackException(
        'Feedback client was closed',
        code: 'disposed',
      );
    }
    if (_token != null &&
        _token!.expiresAt.isAfter(
          DateTime.now().add(const Duration(seconds: 30)),
        )) {
      return _token!;
    }

    if (_refreshing != null) return _refreshing!;
    final generation = _generation;
    final pending = getClientToken().then((token) {
      if (_generation != generation || _disposed) {
        throw const FeedbackException(
          'Feedback identity changed',
          code: 'identity_changed',
        );
      }
      if (token.token.isEmpty || !token.expiresAt.isAfter(DateTime.now())) {
        throw const FeedbackException(
          'Host returned an expired session',
          code: 'invalid_session',
        );
      }

      _token = token;
      return token;
    });
    _refreshing = pending;
    try {
      return await pending;
    } finally {
      if (identical(_refreshing, pending)) _refreshing = null;
    }
  }

  Future<Json> _request(
    String path,
    String method, {
    Json? body,
    String? key,
    bool refresh = true,
  }) async {
    final generation = _generation;
    final token = await _session();

    if (generation != _generation) {
      throw const FeedbackException(
        'Feedback identity changed',
        code: 'identity_changed',
      );
    }

    final request = http.Request(method, base.resolve(path))
      ..followRedirects = false;
    request.headers.addAll({
      'Authorization': 'Bearer ${token.token}',
      'Content-Type': 'application/json',
    });

    if (key != null) request.headers['Idempotency-Key'] = key;

    if (body != null) request.body = jsonEncode(body);
    final response = await _http
        .send(request)
        .timeout(const Duration(seconds: 20));
    final bytes = BytesBuilder();
    await for (final chunk in response.stream.timeout(
      const Duration(seconds: 20),
    )) {
      if (bytes.length + chunk.length > 262144) {
        throw const FeedbackException(
          'Feedback response exceeded its size limit',
          code: 'invalid_response',
        );
      }

      bytes.add(chunk);
    }

    if (generation != _generation) {
      throw const FeedbackException(
        'Feedback identity changed',
        code: 'identity_changed',
      );
    }

    if (response.statusCode == 401 && refresh) {
      if (identical(_token, token)) _token = null;

      return _request(path, method, body: body, key: key, refresh: false);
    }

    Json value;
    try {
      value = jsonDecode(utf8.decode(bytes.takeBytes())) as Json;
    } catch (_) {
      throw FeedbackException(
        'Invalid feedback response',
        code: 'invalid_response',
        status: response.statusCode,
      );
    }

    if (response.statusCode < 200 || response.statusCode >= 300) {
      throw FeedbackException(
        value['message'] as String? ?? 'Feedback request failed',
        code: value['code'] as String? ?? 'request_failed',
        status: response.statusCode,
        retryAfter: int.tryParse(response.headers['retry-after'] ?? ''),
      );
    }

    return value;
  }

  Future<Json> suggest(String text) =>
      _request('suggestions', 'POST', body: {'text': text});
  Future<Json> startConversation(
    String message, {
    String kind = 'feature_request',
  }) {
    final supplied = getContext?.call();
    final context = supplied == null ? null : Json.from(supplied);

    if (context != null && context['route'] is String) {
      context['route'] = (context['route'] as String)
          .split(RegExp(r'[?#]'))
          .first;
    }
    return _request(
      'conversations',
      'POST',
      body: {'message': message, 'kind': kind, 'context': context},
    );
  }

  Future<Json> getConversation(String id) =>
      _request('conversations/${Uri.encodeComponent(id)}', 'GET');
  Future<Json> sendMessage(Json conversation, String body) => _request(
    'conversations/${Uri.encodeComponent(conversation['id'] as String)}/messages',
    'POST',
    body: {'body': body, 'version': conversation['version']},
  );
  Future<Json> submit(String conversation, int version) {
    final scope = '$conversation:$version';
    final key = _submissionKeys.putIfAbsent(
      scope,
      () => 'request-${DateTime.now().microsecondsSinceEpoch}-${_sequence++}',
    );
    return _request(
      'items',
      'POST',
      body: {'conversation': conversation, 'version': version},
      key: key,
    );
  }

  Future<Json> getItem(String id) =>
      _request('items/${Uri.encodeComponent(id)}', 'GET');
  Future<Json> reply(String id, String body) => _request(
    'items/${Uri.encodeComponent(id)}/messages',
    'POST',
    body: {'body': body},
  );
  Future<Json> vote(String id, {bool on = true}) =>
      _request('items/${Uri.encodeComponent(id)}/vote', on ? 'PUT' : 'DELETE');

  /// Clear both identity and pending replies when the host account changes.
  void reset() {
    _generation++;
    _token = null;
    _refreshing = null;
    _submissionKeys.clear();

    if (_ownsHttp) {
      _http.close();
      _http = http.Client();
    }
  }

  void close() {
    reset();
    _disposed = true;

    if (_ownsHttp) _http.close();
  }
}
