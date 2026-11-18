PRAGMA journal_mode = WAL;
INSERT INTO feedback_notifications (id, project_id, recipient, item_id, notification_json, created_at)
 VALUES (3, 1, 'newcomer@example.com', 'fixture-item-1', '{"kind":"new_item","itemId":"fixture-item-1"}', '2026-11-13T12:00:00.000Z');
