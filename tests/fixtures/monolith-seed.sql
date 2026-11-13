-- A small monolith instance for the migration's tests: one organisation with an owner, an
-- administrator and a guest, a custom role and a group, channels and a direct conversation with
-- messages and attachments, a file tree, calendars with an event, notifications and settings.
-- What is encrypted (a TOTP secret, an attendee's token, the VAPID key) is added by the test, with
-- its own key. Applied on `monolith-schema.sql`, whose triggers make the system roles and the
-- Administrator assignment, as the monolith's routes relied on.

INSERT INTO users (id, email, display_name, password_hash, status, locale, timezone, manual_presence, avatar_key) VALUES
  ('00000000-0000-4000-8000-00000000030a', 'ana@example.org', 'Ana', '$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA', 'active', 'fr', 'Europe/Paris', 'dnd', 'avatars/ana'),
  ('00000000-0000-4000-8000-00000000030b', 'bob@example.org', 'Bob', '$argon2id$v=19$m=19456,t=2,p=1$c2FsdA$aGFzaA', 'active', 'en', NULL, NULL, NULL),
  ('00000000-0000-4000-8000-00000000030c', 'cleo@example.org', 'Cleo', NULL, 'active', NULL, NULL, NULL, NULL);
INSERT INTO recovery_codes (id, user_id, code_hash) VALUES
  ('00000000-0000-4000-8000-000000000631', '00000000-0000-4000-8000-00000000030a', '\x0102');
INSERT INTO webauthn_credentials (id, user_id, credential_id, public_key, sign_count, label) VALUES
  ('00000000-0000-4000-8000-000000000771', '00000000-0000-4000-8000-00000000030b', '\x0a0b', '\x0c0d', 3, 'Phone');

INSERT INTO spaces (id, name, slug, created_by, icon_key) VALUES
  ('00000000-0000-4000-8000-000000000501', 'Atelier', 'atelier', '00000000-0000-4000-8000-00000000030a', 'icons/atelier');
INSERT INTO space_slugs (slug, space_id) VALUES ('vieil-atelier', '00000000-0000-4000-8000-000000000501');
INSERT INTO space_members (space_id, user_id, role) VALUES
  ('00000000-0000-4000-8000-000000000501', '00000000-0000-4000-8000-00000000030a', 'owner'),
  ('00000000-0000-4000-8000-000000000501', '00000000-0000-4000-8000-00000000030b', 'admin');
INSERT INTO space_members (space_id, user_id, role, expires_at) VALUES
  ('00000000-0000-4000-8000-000000000501', '00000000-0000-4000-8000-00000000030c', 'guest', now() + interval '30 days');

INSERT INTO space_roles (id, space_id, name, colour, position, permissions) VALUES
  ('00000000-0000-4000-8000-000000000721', '00000000-0000-4000-8000-000000000501', 'Rédaction', 'mint', 10,
   ARRAY['files.manage', 'channels.create', 'calendar.manage', 'members.invite', 'audit.view']);
INSERT INTO space_groups (id, space_id, name) VALUES
  ('00000000-0000-4000-8000-000000000671', '00000000-0000-4000-8000-000000000501', 'Équipe');
INSERT INTO space_group_members (group_id, space_id, user_id) VALUES
  ('00000000-0000-4000-8000-000000000671', '00000000-0000-4000-8000-000000000501', '00000000-0000-4000-8000-00000000030b');
INSERT INTO space_group_roles (group_id, space_id, role_id) VALUES
  ('00000000-0000-4000-8000-000000000671', '00000000-0000-4000-8000-000000000501', '00000000-0000-4000-8000-000000000721');

INSERT INTO conversations (id, space_id, kind) VALUES
  ('00000000-0000-4000-8000-000000000681', '00000000-0000-4000-8000-000000000501', 'channel'),
  ('00000000-0000-4000-8000-000000000682', '00000000-0000-4000-8000-000000000501', 'channel'),
  ('00000000-0000-4000-8000-000000000641', '00000000-0000-4000-8000-000000000501', 'direct');
INSERT INTO channels (id, space_id, name, type, topic, created_by, position) VALUES
  ('00000000-0000-4000-8000-000000000681', '00000000-0000-4000-8000-000000000501', 'général', 'public', 'Tout', '00000000-0000-4000-8000-00000000030a', 0),
  ('00000000-0000-4000-8000-000000000682', '00000000-0000-4000-8000-000000000501', 'secret', 'private', NULL, '00000000-0000-4000-8000-00000000030a', 1);
UPDATE spaces SET default_channel_id = '00000000-0000-4000-8000-000000000681';
INSERT INTO channel_members (channel_id, user_id, role) VALUES
  ('00000000-0000-4000-8000-000000000681', '00000000-0000-4000-8000-00000000030a', 'owner'),
  ('00000000-0000-4000-8000-000000000681', '00000000-0000-4000-8000-00000000030b', 'member'),
  ('00000000-0000-4000-8000-000000000682', '00000000-0000-4000-8000-00000000030a', 'owner');
INSERT INTO channel_role_access (channel_id, role) VALUES ('00000000-0000-4000-8000-000000000682', 'member');
INSERT INTO dm_conversations (id, space_id, created_by) VALUES
  ('00000000-0000-4000-8000-000000000641', '00000000-0000-4000-8000-000000000501', '00000000-0000-4000-8000-00000000030b');
INSERT INTO dm_participants (dm_id, user_id) VALUES
  ('00000000-0000-4000-8000-000000000641', '00000000-0000-4000-8000-00000000030a'),
  ('00000000-0000-4000-8000-000000000641', '00000000-0000-4000-8000-00000000030b');

INSERT INTO space_invitations (id, space_id, token_hash, email, role, created_by) VALUES
  ('00000000-0000-4000-8000-000000000691', '00000000-0000-4000-8000-000000000501', 'aa11', 'dan@example.org', 'admin', '00000000-0000-4000-8000-00000000030a');
INSERT INTO space_invitations (id, space_id, token_hash, role, created_by, channel_id) VALUES
  ('00000000-0000-4000-8000-000000000692', '00000000-0000-4000-8000-000000000501', 'bb22', 'member', '00000000-0000-4000-8000-00000000030a', '00000000-0000-4000-8000-000000000681');
INSERT INTO space_audit_log (id, space_id, actor_id, action, target_kind, target_id) VALUES
  ('00000000-0000-4000-8000-000000000611', '00000000-0000-4000-8000-000000000501', '00000000-0000-4000-8000-00000000030a', 'space.renamed', 'space', '00000000-0000-4000-8000-000000000501'),
  ('00000000-0000-4000-8000-000000000612', '00000000-0000-4000-8000-000000000501', '00000000-0000-4000-8000-00000000030a', 'member.joined', 'member', '00000000-0000-4000-8000-00000000030b'),
  ('00000000-0000-4000-8000-000000000613', '00000000-0000-4000-8000-000000000501', '00000000-0000-4000-8000-00000000030a', 'channel.created', 'channel', '00000000-0000-4000-8000-000000000681');

-- Files: a folder holding a document (two versions, a thumbnail), a file of the direct
-- conversation, and a file put on an event.
INSERT INTO files (id, space_id, owner_id, name, kind) VALUES
  ('00000000-0000-4000-8000-000000000660', '00000000-0000-4000-8000-000000000501', '00000000-0000-4000-8000-00000000030a', 'Documents', 'folder');
INSERT INTO files (id, space_id, owner_id, name, kind, parent_folder_id, size_bytes) VALUES
  ('00000000-0000-4000-8000-000000000661', '00000000-0000-4000-8000-000000000501', '00000000-0000-4000-8000-00000000030a', 'plan.png', 'image', '00000000-0000-4000-8000-000000000660', 5);
INSERT INTO files (id, space_id, owner_id, name, kind, conversation_id, size_bytes) VALUES
  ('00000000-0000-4000-8000-000000000662', '00000000-0000-4000-8000-000000000501', '00000000-0000-4000-8000-00000000030b', 'devis.pdf', 'file', '00000000-0000-4000-8000-000000000641', 4);
INSERT INTO files (id, space_id, owner_id, name, kind, size_bytes) VALUES
  ('00000000-0000-4000-8000-000000000663', '00000000-0000-4000-8000-000000000501', '00000000-0000-4000-8000-00000000030a', 'ordre-du-jour.txt', 'file-text', 3);
INSERT INTO file_versions (id, file_id, version_no, size_bytes, storage_key, thumbnail_key, mime_type, created_by) VALUES
  ('00000000-0000-4000-8000-000000000760', '00000000-0000-4000-8000-000000000661', 1, 4, 'spaces/00000000-0000-4000-8000-000000000501/00000000-0000-4000-8000-000000000661/00000000-0000-4000-8000-000000000760', NULL, 'image/png', '00000000-0000-4000-8000-00000000030a'),
  ('00000000-0000-4000-8000-000000000761', '00000000-0000-4000-8000-000000000661', 2, 5, 'spaces/00000000-0000-4000-8000-000000000501/00000000-0000-4000-8000-000000000661/00000000-0000-4000-8000-000000000761', 'spaces/00000000-0000-4000-8000-000000000501/00000000-0000-4000-8000-000000000661/00000000-0000-4000-8000-000000000761.thumb', 'image/png', '00000000-0000-4000-8000-00000000030a'),
  ('00000000-0000-4000-8000-000000000762', '00000000-0000-4000-8000-000000000662', 1, 4, 'spaces/00000000-0000-4000-8000-000000000501/00000000-0000-4000-8000-000000000662/00000000-0000-4000-8000-000000000762', NULL, 'application/pdf', '00000000-0000-4000-8000-00000000030b'),
  ('00000000-0000-4000-8000-000000000763', '00000000-0000-4000-8000-000000000663', 1, 3, 'spaces/00000000-0000-4000-8000-000000000501/00000000-0000-4000-8000-000000000663/00000000-0000-4000-8000-000000000763', NULL, 'text/plain', '00000000-0000-4000-8000-00000000030a');
UPDATE files SET current_version_id = '00000000-0000-4000-8000-000000000761' WHERE id = '00000000-0000-4000-8000-000000000661';
UPDATE files SET current_version_id = '00000000-0000-4000-8000-000000000762' WHERE id = '00000000-0000-4000-8000-000000000662';
UPDATE files SET current_version_id = '00000000-0000-4000-8000-000000000763' WHERE id = '00000000-0000-4000-8000-000000000663';
INSERT INTO file_stars (user_id, file_id, created_at) VALUES ('00000000-0000-4000-8000-00000000030b', '00000000-0000-4000-8000-000000000661', now());
INSERT INTO file_links (id, file_id, space_id, token, created_by, created_at) VALUES
  ('00000000-0000-4000-8000-0000000006c1', '00000000-0000-4000-8000-000000000661', '00000000-0000-4000-8000-000000000501', 'public-token-1', '00000000-0000-4000-8000-00000000030a', now());
INSERT INTO file_shares (id, file_id, shared_by, target_user_id, permission) VALUES
  ('00000000-0000-4000-8000-000000000731', '00000000-0000-4000-8000-000000000661', '00000000-0000-4000-8000-00000000030a', '00000000-0000-4000-8000-00000000030c', 'view');
INSERT INTO permission_overrides (id, space_id, file_id, user_id, allow, deny) VALUES
  ('00000000-0000-4000-8000-0000000006f1', '00000000-0000-4000-8000-000000000501', '00000000-0000-4000-8000-000000000661', '00000000-0000-4000-8000-00000000030c', ARRAY['files.view'], ARRAY[]::text[]);
INSERT INTO permission_overrides (id, space_id, file_id, audience_channel_id, allow, deny) VALUES
  ('00000000-0000-4000-8000-0000000006f2', '00000000-0000-4000-8000-000000000501', '00000000-0000-4000-8000-000000000660', '00000000-0000-4000-8000-000000000681', ARRAY['files.view'], ARRAY[]::text[]);

-- Messages: a mention and its reply in the channel, a file in the direct conversation, the
-- folder's document attached to two messages of the channel.
INSERT INTO messages (id, conversation_id, author_id, kind, body, created_at) VALUES
  ('00000000-0000-4000-8000-0000000006d1', '00000000-0000-4000-8000-000000000681', '00000000-0000-4000-8000-00000000030a', 'message', 'Bonjour @Bob, voici le **plan**', now() - interval '5 minutes'),
  ('00000000-0000-4000-8000-0000000006d3', '00000000-0000-4000-8000-000000000641', '00000000-0000-4000-8000-00000000030b', 'message', 'Le devis', now() - interval '3 minutes'),
  ('00000000-0000-4000-8000-0000000006d4', '00000000-0000-4000-8000-000000000681', '00000000-0000-4000-8000-00000000030a', 'message', 'Encore le plan', now() - interval '2 minutes');
INSERT INTO messages (id, conversation_id, author_id, kind, body, parent_message_id, created_at) VALUES
  ('00000000-0000-4000-8000-0000000006d2', '00000000-0000-4000-8000-000000000681', '00000000-0000-4000-8000-00000000030b', 'message', 'Merci !', '00000000-0000-4000-8000-0000000006d1', now() - interval '4 minutes');
UPDATE messages SET reply_count = 1 WHERE id = '00000000-0000-4000-8000-0000000006d1';
INSERT INTO message_attachments (message_id, file_id, position, alt_text) VALUES
  ('00000000-0000-4000-8000-0000000006d1', '00000000-0000-4000-8000-000000000661', 0, 'Le plan'),
  ('00000000-0000-4000-8000-0000000006d3', '00000000-0000-4000-8000-000000000662', 0, NULL),
  ('00000000-0000-4000-8000-0000000006d4', '00000000-0000-4000-8000-000000000661', 0, NULL);
INSERT INTO message_mentions (message_id, mentioned_user_id, mention_type) VALUES
  ('00000000-0000-4000-8000-0000000006d1', '00000000-0000-4000-8000-00000000030b', 'user');
INSERT INTO message_reactions (message_id, user_id, emoji) VALUES
  ('00000000-0000-4000-8000-0000000006d1', '00000000-0000-4000-8000-00000000030b', '👍');
INSERT INTO channel_pins (channel_id, message_id, pinned_by) VALUES
  ('00000000-0000-4000-8000-000000000681', '00000000-0000-4000-8000-0000000006d1', '00000000-0000-4000-8000-00000000030a');
INSERT INTO user_saved_messages (user_id, message_id) VALUES
  ('00000000-0000-4000-8000-00000000030b', '00000000-0000-4000-8000-0000000006d1');
INSERT INTO read_cursors (conversation_id, user_id, last_read_message_id) VALUES
  ('00000000-0000-4000-8000-000000000681', '00000000-0000-4000-8000-00000000030b', '00000000-0000-4000-8000-0000000006d2');
INSERT INTO message_link_previews (id, message_id, url, domain, title, image_key, image_width, image_height) VALUES
  ('00000000-0000-4000-8000-000000000701', '00000000-0000-4000-8000-0000000006d1', 'https://example.org/plan', 'example.org', 'Le plan', 'previews/p1', 2, 2);

-- Calendars: the organisation's, with an event linked to the channel, and Ana's own.
INSERT INTO calendars (id, space_id, name, color, created_by) VALUES
  ('00000000-0000-4000-8000-0000000006b1', '00000000-0000-4000-8000-000000000501', 'Atelier', 'sky', '00000000-0000-4000-8000-00000000030a');
INSERT INTO calendars (id, owner_user_id, name, color, is_default) VALUES
  ('00000000-0000-4000-8000-0000000006b2', '00000000-0000-4000-8000-00000000030a', 'Ana', 'pink', true);
INSERT INTO calendar_events (id, calendar_id, uid, title, all_day, start_at, end_at, tzid, created_by, channel_id) VALUES
  ('00000000-0000-4000-8000-000000000651', '00000000-0000-4000-8000-0000000006b1', 'e1@ruchoir', 'Réunion', false,
   '2026-12-01 09:00:00+00', '2026-12-01 10:00:00+00', 'Europe/Paris', '00000000-0000-4000-8000-00000000030a', '00000000-0000-4000-8000-000000000681');
INSERT INTO calendar_event_attendees (id, event_id, user_id, status, invited_by) VALUES
  ('00000000-0000-4000-8000-000000000741', '00000000-0000-4000-8000-000000000651', '00000000-0000-4000-8000-00000000030b', 'accepted', '00000000-0000-4000-8000-00000000030a');
INSERT INTO calendar_event_reminders (user_id, event_id, minutes) VALUES
  ('00000000-0000-4000-8000-00000000030b', '00000000-0000-4000-8000-000000000651', ARRAY[10]);
INSERT INTO calendar_shares (calendar_id, user_id, level, created_by) VALUES
  ('00000000-0000-4000-8000-0000000006b2', '00000000-0000-4000-8000-00000000030b', 'freebusy', '00000000-0000-4000-8000-00000000030a');
INSERT INTO calendar_feed_tokens (id, user_id, calendar_id, token_hash) VALUES
  ('00000000-0000-4000-8000-000000000711', '00000000-0000-4000-8000-00000000030a', '00000000-0000-4000-8000-0000000006b1', 'cc33');
INSERT INTO calendar_event_files (event_id, file_id, added_by) VALUES
  ('00000000-0000-4000-8000-000000000651', '00000000-0000-4000-8000-000000000663', '00000000-0000-4000-8000-00000000030a');

-- Notifications and settings.
INSERT INTO notifications (id, user_id, kind, conversation_id, message_id, actor_id) VALUES
  ('00000000-0000-4000-8000-0000000006e1', '00000000-0000-4000-8000-00000000030b', 'mention', '00000000-0000-4000-8000-000000000681', '00000000-0000-4000-8000-0000000006d1', '00000000-0000-4000-8000-00000000030a'),
  ('00000000-0000-4000-8000-0000000006e2', '00000000-0000-4000-8000-00000000030a', 'dm', '00000000-0000-4000-8000-000000000641', '00000000-0000-4000-8000-0000000006d3', '00000000-0000-4000-8000-00000000030b');
INSERT INTO notifications (id, user_id, kind, event_id, actor_id, payload) VALUES
  ('00000000-0000-4000-8000-0000000006e3', '00000000-0000-4000-8000-00000000030b', 'calendar_invitation', '00000000-0000-4000-8000-000000000651', '00000000-0000-4000-8000-00000000030a', '{}');
INSERT INTO push_devices (id, user_id, platform, token) VALUES
  ('00000000-0000-4000-8000-000000000751', '00000000-0000-4000-8000-00000000030b', 'android', 'device-token');
INSERT INTO push_subscriptions (id, user_id, endpoint, p256dh, auth) VALUES
  ('00000000-0000-4000-8000-000000000752', '00000000-0000-4000-8000-00000000030a', 'https://push.example.org/1', 'key', 'auth');
INSERT INTO user_preferences (user_id, theme, notifications) VALUES
  ('00000000-0000-4000-8000-00000000030b', 'dark', '{"enabled":true,"mentions":true,"replies":false,"quietHours":true,"quietFrom":"22:00","quietTo":"07:00","emailReplies":false}');
INSERT INTO space_notification_prefs (user_id, space_id, level) VALUES
  ('00000000-0000-4000-8000-00000000030b', '00000000-0000-4000-8000-000000000501', 'mentions');
