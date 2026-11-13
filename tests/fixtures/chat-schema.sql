--
-- PostgreSQL database dump
--


-- Dumped from database version 18.6
-- Dumped by pg_dump version 18.6

SET statement_timeout = 0;
SET lock_timeout = 0;
SET idle_in_transaction_session_timeout = 0;
SET transaction_timeout = 0;
SET client_encoding = 'UTF8';
SET standard_conforming_strings = on;
SELECT pg_catalog.set_config('search_path', '', false);
SET check_function_bodies = false;
SET xmloption = content;
SET client_min_messages = warning;
SET row_security = off;

--
-- Name: pg_trgm; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS pg_trgm WITH SCHEMA public;


--
-- Name: EXTENSION pg_trgm; Type: COMMENT; Schema: -; Owner: -
--

COMMENT ON EXTENSION pg_trgm IS 'text similarity measurement and index searching based on trigrams';


--
-- Name: unaccent; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS unaccent WITH SCHEMA public;


--
-- Name: EXTENSION unaccent; Type: COMMENT; Schema: -; Owner: -
--

COMMENT ON EXTENSION unaccent IS 'text search dictionary that removes accents';


--
-- Name: chat_message_file_gone(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.chat_message_file_gone() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM message_attachments WHERE file_id = OLD.file_id) THEN
        INSERT INTO chat_file_erasures (file_id) VALUES (OLD.file_id)
            ON CONFLICT (file_id) DO NOTHING;
    END IF;
    RETURN NULL;
END;
$$;


--
-- Name: ruchoir_unaccent(text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.ruchoir_unaccent(text) RETURNS text
    LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE
    AS $_$ SELECT public.unaccent('public.unaccent'::regdictionary, $1) $_$;


--
-- Name: fr_unaccent; Type: TEXT SEARCH CONFIGURATION; Schema: public; Owner: -
--

CREATE TEXT SEARCH CONFIGURATION public.fr_unaccent (
    PARSER = pg_catalog."default" );

ALTER TEXT SEARCH CONFIGURATION public.fr_unaccent
    ADD MAPPING FOR asciiword WITH french_stem;

ALTER TEXT SEARCH CONFIGURATION public.fr_unaccent
    ADD MAPPING FOR word WITH public.unaccent, french_stem;

ALTER TEXT SEARCH CONFIGURATION public.fr_unaccent
    ADD MAPPING FOR numword WITH simple;

ALTER TEXT SEARCH CONFIGURATION public.fr_unaccent
    ADD MAPPING FOR email WITH simple;

ALTER TEXT SEARCH CONFIGURATION public.fr_unaccent
    ADD MAPPING FOR url WITH simple;

ALTER TEXT SEARCH CONFIGURATION public.fr_unaccent
    ADD MAPPING FOR host WITH simple;

ALTER TEXT SEARCH CONFIGURATION public.fr_unaccent
    ADD MAPPING FOR sfloat WITH simple;

ALTER TEXT SEARCH CONFIGURATION public.fr_unaccent
    ADD MAPPING FOR version WITH simple;

ALTER TEXT SEARCH CONFIGURATION public.fr_unaccent
    ADD MAPPING FOR hword_numpart WITH simple;

ALTER TEXT SEARCH CONFIGURATION public.fr_unaccent
    ADD MAPPING FOR hword_part WITH public.unaccent, french_stem;

ALTER TEXT SEARCH CONFIGURATION public.fr_unaccent
    ADD MAPPING FOR hword_asciipart WITH french_stem;

ALTER TEXT SEARCH CONFIGURATION public.fr_unaccent
    ADD MAPPING FOR numhword WITH simple;

ALTER TEXT SEARCH CONFIGURATION public.fr_unaccent
    ADD MAPPING FOR asciihword WITH french_stem;

ALTER TEXT SEARCH CONFIGURATION public.fr_unaccent
    ADD MAPPING FOR hword WITH public.unaccent, french_stem;

ALTER TEXT SEARCH CONFIGURATION public.fr_unaccent
    ADD MAPPING FOR url_path WITH simple;

ALTER TEXT SEARCH CONFIGURATION public.fr_unaccent
    ADD MAPPING FOR file WITH simple;

ALTER TEXT SEARCH CONFIGURATION public.fr_unaccent
    ADD MAPPING FOR "float" WITH simple;

ALTER TEXT SEARCH CONFIGURATION public.fr_unaccent
    ADD MAPPING FOR "int" WITH simple;

ALTER TEXT SEARCH CONFIGURATION public.fr_unaccent
    ADD MAPPING FOR uint WITH simple;


SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: channel_members; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.channel_members (
    channel_id uuid NOT NULL,
    user_id uuid NOT NULL,
    role text DEFAULT 'member'::text NOT NULL,
    notification_level text DEFAULT 'default'::text NOT NULL,
    muted boolean DEFAULT false NOT NULL,
    favorite boolean DEFAULT false NOT NULL,
    joined_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT channel_members_notification_level_check CHECK ((notification_level = ANY (ARRAY['default'::text, 'all'::text, 'mentions'::text, 'none'::text]))),
    CONSTRAINT channel_members_role_check CHECK ((role = ANY (ARRAY['member'::text, 'admin'::text, 'owner'::text])))
);


--
-- Name: channel_pins; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.channel_pins (
    channel_id uuid NOT NULL,
    message_id uuid NOT NULL,
    pinned_by uuid,
    pinned_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: channels; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.channels (
    id uuid NOT NULL,
    organisation_id uuid NOT NULL,
    name text NOT NULL,
    type text NOT NULL,
    topic text,
    created_by uuid,
    archived_at timestamp with time zone,
    "position" integer,
    is_default boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT channels_name_check CHECK (((char_length(name) >= 1) AND (char_length(name) <= 64))),
    CONSTRAINT channels_type_check CHECK ((type = ANY (ARRAY['public'::text, 'private'::text, 'archived'::text])))
);


--
-- Name: chat_file_erasures; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.chat_file_erasures (
    file_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    attempts integer DEFAULT 0 NOT NULL,
    next_attempt_at timestamp with time zone DEFAULT now() NOT NULL,
    last_error text
);


--
-- Name: chat_notices; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.chat_notices (
    id uuid NOT NULL,
    account_id uuid NOT NULL,
    message_id uuid NOT NULL,
    kind text NOT NULL,
    actor_id uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: chat_rules; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.chat_rules (
    id uuid NOT NULL,
    organisation_id uuid NOT NULL,
    channel_id uuid NOT NULL,
    subject_kind text NOT NULL,
    subject_id uuid NOT NULL,
    allow text[] DEFAULT '{}'::text[] NOT NULL,
    deny text[] DEFAULT '{}'::text[] NOT NULL,
    created_by uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT chat_rules_subject_kind_check CHECK ((subject_kind = ANY (ARRAY['role'::text, 'group'::text, 'person'::text])))
);


--
-- Name: conversations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.conversations (
    id uuid NOT NULL,
    organisation_id uuid NOT NULL,
    kind text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT conversations_kind_check CHECK ((kind = ANY (ARRAY['channel'::text, 'direct'::text])))
);


--
-- Name: dm_conversations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.dm_conversations (
    id uuid NOT NULL,
    organisation_id uuid NOT NULL,
    is_group boolean DEFAULT false NOT NULL,
    created_by uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: dm_participants; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.dm_participants (
    dm_id uuid NOT NULL,
    user_id uuid NOT NULL,
    notification_level text DEFAULT 'default'::text NOT NULL,
    muted boolean DEFAULT false NOT NULL,
    hidden boolean DEFAULT false NOT NULL,
    added_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT dm_participants_notification_level_check CHECK ((notification_level = ANY (ARRAY['default'::text, 'all'::text, 'mentions'::text, 'none'::text])))
);


--
-- Name: group_members; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.group_members (
    organisation_id uuid NOT NULL,
    group_id uuid NOT NULL,
    account_id uuid NOT NULL
);


--
-- Name: groups; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.groups (
    organisation_id uuid NOT NULL,
    id uuid NOT NULL,
    name text NOT NULL
);


--
-- Name: handled_events; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.handled_events (
    id uuid NOT NULL,
    kind text NOT NULL,
    handled_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: members; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.members (
    organisation_id uuid NOT NULL,
    account_id uuid NOT NULL,
    standing text NOT NULL,
    expires_at timestamp with time zone,
    name text DEFAULT ''::text NOT NULL,
    locale text,
    timezone text,
    CONSTRAINT members_standing_check CHECK ((standing = ANY (ARRAY['owner'::text, 'member'::text, 'guest'::text])))
);


--
-- Name: message_attachments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.message_attachments (
    message_id uuid NOT NULL,
    file_id uuid NOT NULL,
    organisation_id uuid NOT NULL,
    "position" integer NOT NULL,
    alt_text text,
    name text NOT NULL,
    mime_type text NOT NULL,
    size_bytes bigint NOT NULL,
    added_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: message_cards; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.message_cards (
    message_id uuid NOT NULL,
    product text NOT NULL,
    delivery_id uuid,
    event_id uuid NOT NULL,
    card jsonb NOT NULL
);


--
-- Name: message_link_previews; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.message_link_previews (
    id uuid NOT NULL,
    message_id uuid NOT NULL,
    url text NOT NULL,
    domain text NOT NULL,
    title text,
    description text,
    color text,
    image bytea,
    image_width integer,
    image_height integer,
    fetched_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT message_link_previews_image_check CHECK ((octet_length(image) <= 262144))
);


--
-- Name: message_mentions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.message_mentions (
    message_id uuid NOT NULL,
    mentioned_user_id uuid NOT NULL,
    mention_type text NOT NULL,
    CONSTRAINT message_mentions_mention_type_check CHECK ((mention_type = ANY (ARRAY['user'::text, 'here'::text, 'channel'::text, 'group'::text])))
);


--
-- Name: message_reactions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.message_reactions (
    message_id uuid NOT NULL,
    user_id uuid NOT NULL,
    emoji text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: messages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.messages (
    id uuid NOT NULL,
    conversation_id uuid NOT NULL,
    author_id uuid,
    kind text DEFAULT 'message'::text NOT NULL,
    body text DEFAULT ''::text NOT NULL,
    system_event text,
    parent_message_id uuid,
    also_in_channel boolean DEFAULT false NOT NULL,
    reply_count integer DEFAULT 0 NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    edited_at timestamp with time zone,
    deleted_at timestamp with time zone,
    search_vector tsvector GENERATED ALWAYS AS (to_tsvector('public.fr_unaccent'::regconfig, body)) STORED,
    CONSTRAINT messages_kind_check CHECK ((kind = ANY (ARRAY['message'::text, 'system'::text])))
);


--
-- Name: organisations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.organisations (
    id uuid NOT NULL,
    name text NOT NULL,
    synced_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: outbox; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.outbox (
    id uuid NOT NULL,
    kind text NOT NULL,
    body bytea NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    attempts integer DEFAULT 0 NOT NULL,
    next_attempt_at timestamp with time zone DEFAULT now() NOT NULL,
    last_error text,
    sent_at timestamp with time zone
);


--
-- Name: read_cursors; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.read_cursors (
    conversation_id uuid NOT NULL,
    user_id uuid NOT NULL,
    last_read_message_id uuid,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: replica_cursor; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.replica_cursor (
    only_row boolean DEFAULT true NOT NULL,
    seq bigint NOT NULL,
    CONSTRAINT replica_cursor_only_row_check CHECK (only_row)
);


--
-- Name: role_holdings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.role_holdings (
    organisation_id uuid NOT NULL,
    account_id uuid NOT NULL,
    role_id uuid NOT NULL,
    group_id uuid
);


--
-- Name: role_permissions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.role_permissions (
    organisation_id uuid NOT NULL,
    role_id uuid NOT NULL,
    allow text[] DEFAULT '{}'::text[] NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: roles; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.roles (
    organisation_id uuid NOT NULL,
    id uuid NOT NULL,
    system_key text,
    name text NOT NULL,
    "position" integer NOT NULL,
    CONSTRAINT roles_system_key_check CHECK ((system_key = ANY (ARRAY['administrator'::text, 'members'::text, 'guests'::text])))
);


--
-- Name: saved_messages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.saved_messages (
    user_id uuid NOT NULL,
    message_id uuid NOT NULL,
    saved_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: seaql_migrations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.seaql_migrations (
    version character varying NOT NULL,
    applied_at bigint NOT NULL
);


--
-- Name: channel_members channel_members_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.channel_members
    ADD CONSTRAINT channel_members_pkey PRIMARY KEY (channel_id, user_id);


--
-- Name: channel_pins channel_pins_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.channel_pins
    ADD CONSTRAINT channel_pins_pkey PRIMARY KEY (channel_id, message_id);


--
-- Name: channels channels_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.channels
    ADD CONSTRAINT channels_pkey PRIMARY KEY (id);


--
-- Name: chat_file_erasures chat_file_erasures_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_file_erasures
    ADD CONSTRAINT chat_file_erasures_pkey PRIMARY KEY (file_id);


--
-- Name: chat_notices chat_notices_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_notices
    ADD CONSTRAINT chat_notices_pkey PRIMARY KEY (id);


--
-- Name: chat_rules chat_rules_channel_id_subject_kind_subject_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_rules
    ADD CONSTRAINT chat_rules_channel_id_subject_kind_subject_id_key UNIQUE (channel_id, subject_kind, subject_id);


--
-- Name: chat_rules chat_rules_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_rules
    ADD CONSTRAINT chat_rules_pkey PRIMARY KEY (id);


--
-- Name: conversations conversations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.conversations
    ADD CONSTRAINT conversations_pkey PRIMARY KEY (id);


--
-- Name: dm_conversations dm_conversations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.dm_conversations
    ADD CONSTRAINT dm_conversations_pkey PRIMARY KEY (id);


--
-- Name: dm_participants dm_participants_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.dm_participants
    ADD CONSTRAINT dm_participants_pkey PRIMARY KEY (dm_id, user_id);


--
-- Name: group_members group_members_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.group_members
    ADD CONSTRAINT group_members_pkey PRIMARY KEY (organisation_id, group_id, account_id);


--
-- Name: groups groups_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.groups
    ADD CONSTRAINT groups_pkey PRIMARY KEY (organisation_id, id);


--
-- Name: handled_events handled_events_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.handled_events
    ADD CONSTRAINT handled_events_pkey PRIMARY KEY (id);


--
-- Name: members members_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.members
    ADD CONSTRAINT members_pkey PRIMARY KEY (organisation_id, account_id);


--
-- Name: message_attachments message_attachments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_attachments
    ADD CONSTRAINT message_attachments_pkey PRIMARY KEY (message_id, file_id);


--
-- Name: message_cards message_cards_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_cards
    ADD CONSTRAINT message_cards_pkey PRIMARY KEY (message_id);


--
-- Name: message_link_previews message_link_previews_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_link_previews
    ADD CONSTRAINT message_link_previews_pkey PRIMARY KEY (id);


--
-- Name: message_mentions message_mentions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_mentions
    ADD CONSTRAINT message_mentions_pkey PRIMARY KEY (message_id, mentioned_user_id);


--
-- Name: message_reactions message_reactions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_reactions
    ADD CONSTRAINT message_reactions_pkey PRIMARY KEY (message_id, user_id, emoji);


--
-- Name: messages messages_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.messages
    ADD CONSTRAINT messages_pkey PRIMARY KEY (id);


--
-- Name: organisations organisations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.organisations
    ADD CONSTRAINT organisations_pkey PRIMARY KEY (id);


--
-- Name: outbox outbox_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.outbox
    ADD CONSTRAINT outbox_pkey PRIMARY KEY (id);


--
-- Name: read_cursors read_cursors_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.read_cursors
    ADD CONSTRAINT read_cursors_pkey PRIMARY KEY (conversation_id, user_id);


--
-- Name: replica_cursor replica_cursor_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.replica_cursor
    ADD CONSTRAINT replica_cursor_pkey PRIMARY KEY (only_row);


--
-- Name: role_permissions role_permissions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.role_permissions
    ADD CONSTRAINT role_permissions_pkey PRIMARY KEY (organisation_id, role_id);


--
-- Name: roles roles_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.roles
    ADD CONSTRAINT roles_pkey PRIMARY KEY (organisation_id, id);


--
-- Name: saved_messages saved_messages_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.saved_messages
    ADD CONSTRAINT saved_messages_pkey PRIMARY KEY (user_id, message_id);


--
-- Name: seaql_migrations seaql_migrations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.seaql_migrations
    ADD CONSTRAINT seaql_migrations_pkey PRIMARY KEY (version);


--
-- Name: channel_members_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX channel_members_user ON public.channel_members USING btree (user_id);


--
-- Name: channels_author; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX channels_author ON public.channels USING btree (created_by);


--
-- Name: channels_name_per_organisation; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX channels_name_per_organisation ON public.channels USING btree (organisation_id, name);


--
-- Name: channels_one_default_per_organisation; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX channels_one_default_per_organisation ON public.channels USING btree (organisation_id) WHERE is_default;


--
-- Name: chat_file_erasures_due; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX chat_file_erasures_due ON public.chat_file_erasures USING btree (next_attempt_at);


--
-- Name: chat_notices_account; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX chat_notices_account ON public.chat_notices USING btree (account_id);


--
-- Name: chat_notices_actor; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX chat_notices_actor ON public.chat_notices USING btree (actor_id);


--
-- Name: chat_notices_message; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX chat_notices_message ON public.chat_notices USING btree (message_id, account_id);


--
-- Name: chat_rules_subject; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX chat_rules_subject ON public.chat_rules USING btree (organisation_id, subject_kind, subject_id);


--
-- Name: conversations_organisation; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX conversations_organisation ON public.conversations USING btree (organisation_id);


--
-- Name: dm_conversations_organisation; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX dm_conversations_organisation ON public.dm_conversations USING btree (organisation_id);


--
-- Name: dm_participants_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX dm_participants_user ON public.dm_participants USING btree (user_id);


--
-- Name: group_members_account; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX group_members_account ON public.group_members USING btree (organisation_id, account_id);


--
-- Name: idx_handled_events_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_handled_events_at ON public.handled_events USING btree (handled_at);


--
-- Name: idx_outbox_due; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_outbox_due ON public.outbox USING btree (next_attempt_at) WHERE (sent_at IS NULL);


--
-- Name: idx_outbox_sent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_outbox_sent ON public.outbox USING btree (sent_at) WHERE (sent_at IS NOT NULL);


--
-- Name: members_account; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX members_account ON public.members USING btree (account_id);


--
-- Name: message_attachments_file; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX message_attachments_file ON public.message_attachments USING btree (file_id);


--
-- Name: message_cards_delivery; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX message_cards_delivery ON public.message_cards USING btree (product, delivery_id);


--
-- Name: message_cards_event; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX message_cards_event ON public.message_cards USING btree (event_id);


--
-- Name: message_link_previews_message; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX message_link_previews_message ON public.message_link_previews USING btree (message_id);


--
-- Name: message_link_previews_url; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX message_link_previews_url ON public.message_link_previews USING btree (url, fetched_at DESC);


--
-- Name: message_mentions_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX message_mentions_user ON public.message_mentions USING btree (mentioned_user_id);


--
-- Name: message_reactions_recent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX message_reactions_recent ON public.message_reactions USING btree (created_at);


--
-- Name: message_reactions_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX message_reactions_user ON public.message_reactions USING btree (user_id, created_at DESC);


--
-- Name: messages_author; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX messages_author ON public.messages USING btree (author_id);


--
-- Name: messages_body_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX messages_body_trgm ON public.messages USING gin (public.ruchoir_unaccent(body) public.gin_trgm_ops);


--
-- Name: messages_feed; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX messages_feed ON public.messages USING btree (conversation_id, created_at DESC, id DESC);


--
-- Name: messages_search_vector_gin; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX messages_search_vector_gin ON public.messages USING gin (search_vector);


--
-- Name: messages_thread; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX messages_thread ON public.messages USING btree (parent_message_id, created_at) WHERE (parent_message_id IS NOT NULL);


--
-- Name: read_cursors_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX read_cursors_user ON public.read_cursors USING btree (user_id);


--
-- Name: role_holdings_account; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX role_holdings_account ON public.role_holdings USING btree (organisation_id, account_id);


--
-- Name: message_attachments chat_message_file_gone; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER chat_message_file_gone AFTER DELETE ON public.message_attachments FOR EACH ROW EXECUTE FUNCTION public.chat_message_file_gone();


--
-- Name: channel_members channel_members_channel_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.channel_members
    ADD CONSTRAINT channel_members_channel_id_fkey FOREIGN KEY (channel_id) REFERENCES public.channels(id) ON DELETE CASCADE;


--
-- Name: channel_pins channel_pins_channel_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.channel_pins
    ADD CONSTRAINT channel_pins_channel_id_fkey FOREIGN KEY (channel_id) REFERENCES public.channels(id) ON DELETE CASCADE;


--
-- Name: channel_pins channel_pins_message_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.channel_pins
    ADD CONSTRAINT channel_pins_message_id_fkey FOREIGN KEY (message_id) REFERENCES public.messages(id) ON DELETE CASCADE;


--
-- Name: channels channels_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.channels
    ADD CONSTRAINT channels_id_fkey FOREIGN KEY (id) REFERENCES public.conversations(id) ON DELETE CASCADE;


--
-- Name: chat_notices chat_notices_message_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_notices
    ADD CONSTRAINT chat_notices_message_id_fkey FOREIGN KEY (message_id) REFERENCES public.messages(id) ON DELETE CASCADE;


--
-- Name: chat_rules chat_rules_channel_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.chat_rules
    ADD CONSTRAINT chat_rules_channel_id_fkey FOREIGN KEY (channel_id) REFERENCES public.channels(id) ON DELETE CASCADE;


--
-- Name: dm_conversations dm_conversations_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.dm_conversations
    ADD CONSTRAINT dm_conversations_id_fkey FOREIGN KEY (id) REFERENCES public.conversations(id) ON DELETE CASCADE;


--
-- Name: dm_participants dm_participants_dm_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.dm_participants
    ADD CONSTRAINT dm_participants_dm_id_fkey FOREIGN KEY (dm_id) REFERENCES public.dm_conversations(id) ON DELETE CASCADE;


--
-- Name: group_members group_members_organisation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.group_members
    ADD CONSTRAINT group_members_organisation_id_fkey FOREIGN KEY (organisation_id) REFERENCES public.organisations(id) ON DELETE CASCADE;


--
-- Name: groups groups_organisation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.groups
    ADD CONSTRAINT groups_organisation_id_fkey FOREIGN KEY (organisation_id) REFERENCES public.organisations(id) ON DELETE CASCADE;


--
-- Name: members members_organisation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.members
    ADD CONSTRAINT members_organisation_id_fkey FOREIGN KEY (organisation_id) REFERENCES public.organisations(id) ON DELETE CASCADE;


--
-- Name: message_attachments message_attachments_message_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_attachments
    ADD CONSTRAINT message_attachments_message_id_fkey FOREIGN KEY (message_id) REFERENCES public.messages(id) ON DELETE CASCADE;


--
-- Name: message_cards message_cards_message_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_cards
    ADD CONSTRAINT message_cards_message_id_fkey FOREIGN KEY (message_id) REFERENCES public.messages(id) ON DELETE CASCADE;


--
-- Name: message_link_previews message_link_previews_message_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_link_previews
    ADD CONSTRAINT message_link_previews_message_id_fkey FOREIGN KEY (message_id) REFERENCES public.messages(id) ON DELETE CASCADE;


--
-- Name: message_mentions message_mentions_message_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_mentions
    ADD CONSTRAINT message_mentions_message_id_fkey FOREIGN KEY (message_id) REFERENCES public.messages(id) ON DELETE CASCADE;


--
-- Name: message_reactions message_reactions_message_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_reactions
    ADD CONSTRAINT message_reactions_message_id_fkey FOREIGN KEY (message_id) REFERENCES public.messages(id) ON DELETE CASCADE;


--
-- Name: messages messages_conversation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.messages
    ADD CONSTRAINT messages_conversation_id_fkey FOREIGN KEY (conversation_id) REFERENCES public.conversations(id) ON DELETE CASCADE;


--
-- Name: messages messages_parent_message_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.messages
    ADD CONSTRAINT messages_parent_message_id_fkey FOREIGN KEY (parent_message_id) REFERENCES public.messages(id) ON DELETE CASCADE;


--
-- Name: read_cursors read_cursors_conversation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.read_cursors
    ADD CONSTRAINT read_cursors_conversation_id_fkey FOREIGN KEY (conversation_id) REFERENCES public.conversations(id) ON DELETE CASCADE;


--
-- Name: read_cursors read_cursors_last_read_message_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.read_cursors
    ADD CONSTRAINT read_cursors_last_read_message_id_fkey FOREIGN KEY (last_read_message_id) REFERENCES public.messages(id) ON DELETE SET NULL;


--
-- Name: role_holdings role_holdings_organisation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.role_holdings
    ADD CONSTRAINT role_holdings_organisation_id_fkey FOREIGN KEY (organisation_id) REFERENCES public.organisations(id) ON DELETE CASCADE;


--
-- Name: roles roles_organisation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.roles
    ADD CONSTRAINT roles_organisation_id_fkey FOREIGN KEY (organisation_id) REFERENCES public.organisations(id) ON DELETE CASCADE;


--
-- Name: saved_messages saved_messages_message_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.saved_messages
    ADD CONSTRAINT saved_messages_message_id_fkey FOREIGN KEY (message_id) REFERENCES public.messages(id) ON DELETE CASCADE;


--
-- PostgreSQL database dump complete
--


