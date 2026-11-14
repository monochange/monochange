INSERT INTO users (id, github_id, github_login, github_access_token)
 VALUES (4, 77004, 'fixture-post-upgrade', CAST(X'C0DEC0DE1234' AS TEXT));
INSERT INTO oauth_state_uses (nonce, expires_at) VALUES (X'6E6F6E636531323334353637383930', 1792000000);
INSERT INTO client_sessions (id, user_id, token_hash, client, session_epoch, created_at, last_used_at, expires_at)
 VALUES (X'73657373696F6E3132333435363738', 4, X'AABBCCDD0011EE2233445566', 'extension', 0, 1791000000, NULL, 1792000000);
