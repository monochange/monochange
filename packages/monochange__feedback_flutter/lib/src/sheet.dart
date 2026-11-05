import 'dart:async';
import 'package:flutter/material.dart';
import 'client.dart';

/// Embed in a page or show through `showModalBottomSheet` using native Flutter UI.
class FeedbackSheet extends StatefulWidget {
  final FeedbackClient client;
  final String identityKey;
  final VoidCallback? onClose;
  final ValueChanged<Json>? onSubmitted;
  final Map<String, String> labels;
  const FeedbackSheet({
    super.key,
    required this.client,
    required this.identityKey,
    this.onClose,
    this.onSubmitted,
    this.labels = const {},
  });

  @override
  State<FeedbackSheet> createState() => _FeedbackSheetState();
}

class _FeedbackSheetState extends State<FeedbackSheet>
    with WidgetsBindingObserver {
  final _message = TextEditingController();
  Json? _conversation;
  Json? _item;
  List<Json> _suggestions = [];
  String _kind = 'feature_request';
  String? _error;
  bool _busy = false;
  int _generation = 0;
  Timer? _poll;

  String label(String key, String fallback) => widget.labels[key] ?? fallback;

  @override
  void initState() {
    super.initState();
    WidgetsBinding.instance.addObserver(this);
    _poll = Timer.periodic(const Duration(seconds: 30), (_) {
      if (_item != null && !_busy) unawaited(_run(_refresh));
    });
  }

  @override
  void didUpdateWidget(FeedbackSheet oldWidget) {
    super.didUpdateWidget(oldWidget);
    if (oldWidget.client != widget.client ||
        oldWidget.identityKey != widget.identityKey) {
      oldWidget.client.reset();
      _reset();
    }
  }

  @override
  void didChangeAppLifecycleState(AppLifecycleState state) {
    if (state == AppLifecycleState.resumed && _item != null) {
      unawaited(_run(_refresh));
    }
  }

  @override
  void dispose() {
    _generation++;
    WidgetsBinding.instance.removeObserver(this);
    _poll?.cancel();
    _message.dispose();
    super.dispose();
  }

  void _reset() {
    _generation++;
    widget.client.reset();
    setState(() {
      _conversation = null;
      _item = null;
      _suggestions = [];
      _error = null;
      _busy = false;
      _message.clear();
    });
  }

  Future<void> _run(Future<void> Function(bool Function()) action) async {
    if (_busy) return;
    final generation = _generation;
    bool active() => mounted && generation == _generation;
    setState(() {
      _busy = true;
      _error = null;
    });
    try {
      await action(active);
    } catch (error) {
      if (active()) {
        setState(() {
          _error = error.toString();
        });
      }
    } finally {
      if (active()) {
        setState(() {
          _busy = false;
        });
      }
    }
  }

  Future<void> _refresh(bool Function() active) async {
    final item = _item;

    if (item == null) return;
    final latest = await widget.client.getItem(item['id'] as String);

    if (active()) {
      setState(() {
        _item = latest;
      });
    }
  }

  Future<void> _send(bool Function() active) async {
    final body = _message.text.trim();

    if (body.isEmpty) return;

    if (_item != null) {
      await widget.client.reply(_item!['id'] as String, body);
      await _refresh(active);
    } else {
      final result = _conversation == null
          ? await widget.client.startConversation(body, kind: _kind)
          : await widget.client.sendMessage(_conversation!, body);

      if (active()) {
        setState(() {
          _conversation = result['conversation'] as Json;
          _suggestions = (result['similar'] as List).cast<Json>();
        });
      }
    }

    if (active()) _message.clear();
  }

  Future<void> _submit(bool Function() active) async {
    if (_conversation == null) return;
    final receipt = await widget.client.submit(
      _conversation!['id'] as String,
      _conversation!['version'] as int,
    );
    final item = await widget.client.getItem(receipt['id'] as String);

    if (active()) {
      setState(() {
        _item = item;
        _suggestions = [];
      });
      widget.onSubmitted?.call(receipt);
    }
  }

  @override
  Widget build(BuildContext context) {
    final messages =
        (_item?['messages'] ?? _conversation?['messages'] ?? []) as List;
    return SafeArea(
      child: Padding(
        padding: EdgeInsets.fromLTRB(
          20,
          16,
          20,
          16 + MediaQuery.viewInsetsOf(context).bottom,
        ),
        child: SingleChildScrollView(
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.stretch,
            mainAxisSize: MainAxisSize.min,
            children: [
              Row(
                children: [
                  Expanded(
                    child: Semantics(
                      header: true,
                      child: Text(
                        label('title', 'Feedback'),
                        style: Theme.of(context).textTheme.headlineSmall,
                      ),
                    ),
                  ),
                  if (widget.onClose != null)
                    IconButton(
                      tooltip: label('close', 'Close'),
                      onPressed: widget.onClose,
                      icon: const Icon(Icons.close),
                    ),
                ],
              ),
              if (_error != null)
                Semantics(
                  liveRegion: true,
                  child: Text(
                    _error!,
                    style: TextStyle(
                      color: Theme.of(context).colorScheme.error,
                    ),
                  ),
                ),
              if (_item != null)
                Semantics(
                  liveRegion: true,
                  child: Text('${_item!['title']} · ${_item!['status']}'),
                ),
              for (final message in messages)
                Card(
                  child: Padding(
                    padding: const EdgeInsets.all(12),
                    child: Text(
                      '${message['role'] ?? message['author']}: ${message['body']}',
                    ),
                  ),
                ),
              for (final suggestion in _suggestions)
                Card(
                  child: Padding(
                    padding: const EdgeInsets.all(12),
                    child: Column(
                      crossAxisAlignment: CrossAxisAlignment.stretch,
                      children: [
                        Text(
                          suggestion['title'] as String,
                          style: Theme.of(context).textTheme.titleMedium,
                        ),
                        Text(
                          '${suggestion['status']} · ${suggestion['votes']}',
                        ),
                        Semantics(
                          toggled: suggestion['voted'] == true,
                          child: OutlinedButton(
                            onPressed: _busy
                                ? null
                                : () => unawaited(
                                    _run((active) async {
                                      final vote = await widget.client.vote(
                                        suggestion['id'] as String,
                                        on: suggestion['voted'] != true,
                                      );
                                      if (active()) {
                                        setState(() {
                                          suggestion.addAll(vote);
                                        });
                                      }
                                    }),
                                  ),
                            child: Text(
                              suggestion['voted'] == true
                                  ? label('voted', 'Upvoted')
                                  : label('upvote', 'Upvote'),
                            ),
                          ),
                        ),
                      ],
                    ),
                  ),
                ),
              if (_conversation == null && _item == null)
                DropdownButtonFormField<String>(
                  initialValue: _kind,
                  decoration: InputDecoration(
                    labelText: label('kind', 'Request type'),
                  ),
                  items: [
                    DropdownMenuItem(
                      value: 'feature_request',
                      child: Text(label('feature', 'Feature or change')),
                    ),
                    DropdownMenuItem(
                      value: 'bug_report',
                      child: Text(label('bug', 'Bug report')),
                    ),
                  ],
                  onChanged: _busy
                      ? null
                      : (kind) {
                          if (kind != null) {
                            setState(() {
                              _kind = kind;
                            });
                          }
                        },
                ),
              const SizedBox(height: 12),
              TextField(
                controller: _message,
                enabled: !_busy,
                maxLength: 4000,
                minLines: 3,
                maxLines: 6,
                decoration: InputDecoration(
                  labelText: label('message', 'What would you like to change?'),
                  border: const OutlineInputBorder(),
                ),
              ),
              FilledButton(
                onPressed: _busy ? null : () => unawaited(_run(_send)),
                child: Text(
                  _busy
                      ? label('working', 'Working…')
                      : _conversation != null || _item != null
                      ? label('reply', 'Send reply')
                      : label('send', 'Find similar requests'),
                ),
              ),
              if (_conversation != null && _item == null)
                OutlinedButton(
                  onPressed: _busy ? null : () => unawaited(_run(_submit)),
                  child: Text(label('submit', 'Submit a new request')),
                ),
              if (_item != null)
                TextButton(
                  onPressed: _busy ? null : _reset,
                  child: Text(label('another', 'Start another request')),
                ),
            ],
          ),
        ),
      ),
    );
  }
}
