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
-- Name: unaccent; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS unaccent WITH SCHEMA public;


--
-- Name: EXTENSION unaccent; Type: COMMENT; Schema: -; Owner: -
--

COMMENT ON EXTENSION unaccent IS 'text search dictionary that removes accents';


--
-- Name: calendar_event_file_gone(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.calendar_event_file_gone() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM calendar_event_files WHERE file_id = OLD.file_id) THEN
        INSERT INTO calendar_file_erasures (file_id) VALUES (OLD.file_id)
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


SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: calendar_attendee_overrides; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendar_attendee_overrides (
    attendee_id uuid NOT NULL,
    recurrence_id text NOT NULL,
    status text NOT NULL,
    CONSTRAINT calendar_attendee_overrides_status_check CHECK ((status = ANY (ARRAY['needs_action'::text, 'accepted'::text, 'tentative'::text, 'declined'::text])))
);


--
-- Name: calendar_chat_cards; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendar_chat_cards (
    id uuid NOT NULL,
    channel_ref text NOT NULL,
    kind text NOT NULL,
    event_id uuid NOT NULL,
    card jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    attempts integer DEFAULT 0 NOT NULL,
    next_attempt_at timestamp with time zone DEFAULT now() NOT NULL,
    last_error text
);


--
-- Name: calendar_event_attendees; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendar_event_attendees (
    id uuid NOT NULL,
    event_id uuid NOT NULL,
    user_id uuid,
    status text DEFAULT 'needs_action'::text NOT NULL,
    invited_by uuid,
    invited_at timestamp with time zone DEFAULT now() NOT NULL,
    responded_at timestamp with time zone,
    email text,
    name text,
    token_digest text,
    token_cipher bytea,
    token_nonce bytea,
    CONSTRAINT calendar_event_attendees_status_check CHECK ((status = ANY (ARRAY['needs_action'::text, 'accepted'::text, 'tentative'::text, 'declined'::text]))),
    CONSTRAINT calendar_event_attendees_token CHECK ((((email IS NULL) = (token_digest IS NULL)) AND ((token_digest IS NULL) = (token_cipher IS NULL)) AND ((token_cipher IS NULL) = (token_nonce IS NULL)))),
    CONSTRAINT calendar_event_attendees_who CHECK (((user_id IS NULL) <> (email IS NULL)))
);


--
-- Name: calendar_event_exceptions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendar_event_exceptions (
    event_id uuid NOT NULL,
    recurrence_id text NOT NULL,
    cancelled boolean DEFAULT false NOT NULL,
    title text,
    description text,
    location text,
    start_at timestamp with time zone,
    end_at timestamp with time zone,
    start_date date,
    end_date date,
    ical_extra jsonb DEFAULT '[]'::jsonb NOT NULL
);


--
-- Name: calendar_event_files; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendar_event_files (
    event_id uuid NOT NULL,
    file_id uuid NOT NULL,
    organisation_id uuid NOT NULL,
    name text NOT NULL,
    added_by uuid,
    added_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: calendar_event_reminders; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendar_event_reminders (
    user_id uuid NOT NULL,
    event_id uuid NOT NULL,
    minutes integer[] NOT NULL
);


--
-- Name: calendar_events; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendar_events (
    id uuid NOT NULL,
    calendar_id uuid NOT NULL,
    uid text NOT NULL,
    title text NOT NULL,
    description text,
    location text,
    all_day boolean NOT NULL,
    start_at timestamp with time zone,
    end_at timestamp with time zone,
    tzid text,
    start_date date,
    end_date date,
    rrule text,
    rdates timestamp with time zone[],
    series_until timestamp with time zone,
    sequence integer DEFAULT 0 NOT NULL,
    ical_extra jsonb DEFAULT '[]'::jsonb NOT NULL,
    created_by uuid,
    updated_by uuid,
    channel_ref text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT calendar_events_when_check CHECK (((all_day AND (start_date IS NOT NULL) AND (end_date IS NOT NULL) AND (end_date > start_date) AND (start_at IS NULL) AND (end_at IS NULL) AND (tzid IS NULL)) OR ((NOT all_day) AND (start_at IS NOT NULL) AND (end_at IS NOT NULL) AND (end_at >= start_at) AND (tzid IS NOT NULL) AND (start_date IS NULL) AND (end_date IS NULL))))
);


--
-- Name: calendar_feed_tokens; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendar_feed_tokens (
    id uuid NOT NULL,
    user_id uuid NOT NULL,
    calendar_id uuid,
    token_digest text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    last_used_at timestamp with time zone,
    revoked_at timestamp with time zone
);


--
-- Name: calendar_file_erasures; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendar_file_erasures (
    file_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    attempts integer DEFAULT 0 NOT NULL,
    next_attempt_at timestamp with time zone DEFAULT now() NOT NULL,
    last_error text
);


--
-- Name: calendar_reminder_deliveries; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendar_reminder_deliveries (
    user_id uuid NOT NULL,
    event_id uuid NOT NULL,
    occurrence_start timestamp with time zone NOT NULL,
    sent_at timestamp with time zone NOT NULL,
    minutes integer
);


--
-- Name: calendar_reminder_prefs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendar_reminder_prefs (
    user_id uuid NOT NULL,
    calendar_id uuid NOT NULL,
    minutes integer[] NOT NULL
);


--
-- Name: calendar_rules; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendar_rules (
    id uuid NOT NULL,
    organisation_id uuid NOT NULL,
    calendar_id uuid NOT NULL,
    subject_kind text NOT NULL,
    subject_id uuid NOT NULL,
    allow text[] DEFAULT '{}'::text[] NOT NULL,
    deny text[] DEFAULT '{}'::text[] NOT NULL,
    created_by uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT calendar_rules_subject_kind_check CHECK ((subject_kind = ANY (ARRAY['role'::text, 'group'::text, 'person'::text])))
);


--
-- Name: calendar_shares; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendar_shares (
    calendar_id uuid NOT NULL,
    user_id uuid NOT NULL,
    level text NOT NULL,
    created_by uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT calendar_shares_level_check CHECK ((level = ANY (ARRAY['freebusy'::text, 'read'::text, 'write'::text])))
);


--
-- Name: calendar_visibility; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendar_visibility (
    user_id uuid NOT NULL,
    calendar_id uuid NOT NULL,
    hidden boolean NOT NULL
);


--
-- Name: calendars; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendars (
    id uuid NOT NULL,
    organisation_id uuid,
    owner_user_id uuid,
    name text NOT NULL,
    description text,
    color text NOT NULL,
    default_reminders integer[] DEFAULT '{}'::integer[] NOT NULL,
    is_default boolean DEFAULT false NOT NULL,
    created_by uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT calendars_color_check CHECK ((color = ANY (ARRAY['sky'::text, 'mint'::text, 'violet'::text, 'pink'::text, 'peach'::text, 'lime'::text, 'sun'::text, 'accent'::text]))),
    CONSTRAINT calendars_name_check CHECK (((char_length(name) >= 1) AND (char_length(name) <= 200))),
    CONSTRAINT calendars_owner_check CHECK (((organisation_id IS NULL) <> (owner_user_id IS NULL)))
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
    locale text,
    timezone text,
    CONSTRAINT members_standing_check CHECK ((standing = ANY (ARRAY['owner'::text, 'member'::text, 'guest'::text])))
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
-- Name: seaql_migrations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.seaql_migrations (
    version character varying NOT NULL,
    applied_at bigint NOT NULL
);


--
-- Name: calendar_attendee_overrides calendar_attendee_overrides_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_attendee_overrides
    ADD CONSTRAINT calendar_attendee_overrides_pkey PRIMARY KEY (attendee_id, recurrence_id);


--
-- Name: calendar_chat_cards calendar_chat_cards_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_chat_cards
    ADD CONSTRAINT calendar_chat_cards_pkey PRIMARY KEY (id);


--
-- Name: calendar_event_attendees calendar_event_attendees_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_event_attendees
    ADD CONSTRAINT calendar_event_attendees_pkey PRIMARY KEY (id);


--
-- Name: calendar_event_exceptions calendar_event_exceptions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_event_exceptions
    ADD CONSTRAINT calendar_event_exceptions_pkey PRIMARY KEY (event_id, recurrence_id);


--
-- Name: calendar_event_files calendar_event_files_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_event_files
    ADD CONSTRAINT calendar_event_files_pkey PRIMARY KEY (event_id, file_id);


--
-- Name: calendar_event_reminders calendar_event_reminders_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_event_reminders
    ADD CONSTRAINT calendar_event_reminders_pkey PRIMARY KEY (user_id, event_id);


--
-- Name: calendar_events calendar_events_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_events
    ADD CONSTRAINT calendar_events_pkey PRIMARY KEY (id);


--
-- Name: calendar_events calendar_events_uid; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_events
    ADD CONSTRAINT calendar_events_uid UNIQUE (calendar_id, uid);


--
-- Name: calendar_feed_tokens calendar_feed_tokens_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_feed_tokens
    ADD CONSTRAINT calendar_feed_tokens_pkey PRIMARY KEY (id);


--
-- Name: calendar_feed_tokens calendar_feed_tokens_token_digest_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_feed_tokens
    ADD CONSTRAINT calendar_feed_tokens_token_digest_key UNIQUE (token_digest);


--
-- Name: calendar_file_erasures calendar_file_erasures_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_file_erasures
    ADD CONSTRAINT calendar_file_erasures_pkey PRIMARY KEY (file_id);


--
-- Name: calendar_reminder_prefs calendar_reminder_prefs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_reminder_prefs
    ADD CONSTRAINT calendar_reminder_prefs_pkey PRIMARY KEY (user_id, calendar_id);


--
-- Name: calendar_rules calendar_rules_calendar_id_subject_kind_subject_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_rules
    ADD CONSTRAINT calendar_rules_calendar_id_subject_kind_subject_id_key UNIQUE (calendar_id, subject_kind, subject_id);


--
-- Name: calendar_rules calendar_rules_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_rules
    ADD CONSTRAINT calendar_rules_pkey PRIMARY KEY (id);


--
-- Name: calendar_shares calendar_shares_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_shares
    ADD CONSTRAINT calendar_shares_pkey PRIMARY KEY (calendar_id, user_id);


--
-- Name: calendar_visibility calendar_visibility_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_visibility
    ADD CONSTRAINT calendar_visibility_pkey PRIMARY KEY (user_id, calendar_id);


--
-- Name: calendars calendars_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendars
    ADD CONSTRAINT calendars_pkey PRIMARY KEY (id);


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
-- Name: seaql_migrations seaql_migrations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.seaql_migrations
    ADD CONSTRAINT seaql_migrations_pkey PRIMARY KEY (version);


--
-- Name: calendar_chat_cards_due; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_chat_cards_due ON public.calendar_chat_cards USING btree (next_attempt_at);


--
-- Name: calendar_event_attendees_email; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX calendar_event_attendees_email ON public.calendar_event_attendees USING btree (event_id, lower(email)) WHERE (email IS NOT NULL);


--
-- Name: calendar_event_attendees_person; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_event_attendees_person ON public.calendar_event_attendees USING btree (user_id);


--
-- Name: calendar_event_attendees_token; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX calendar_event_attendees_token ON public.calendar_event_attendees USING btree (token_digest) WHERE (token_digest IS NOT NULL);


--
-- Name: calendar_event_attendees_user; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX calendar_event_attendees_user ON public.calendar_event_attendees USING btree (event_id, user_id) WHERE (user_id IS NOT NULL);


--
-- Name: calendar_event_files_file; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_event_files_file ON public.calendar_event_files USING btree (file_id);


--
-- Name: calendar_events_author; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_events_author ON public.calendar_events USING btree (created_by);


--
-- Name: calendar_events_series_until; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_events_series_until ON public.calendar_events USING btree (calendar_id, series_until);


--
-- Name: calendar_events_start_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_events_start_at ON public.calendar_events USING btree (calendar_id, start_at);


--
-- Name: calendar_events_start_date; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_events_start_date ON public.calendar_events USING btree (calendar_id, start_date);


--
-- Name: calendar_feed_tokens_calendar; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_feed_tokens_calendar ON public.calendar_feed_tokens USING btree (calendar_id) WHERE (calendar_id IS NOT NULL);


--
-- Name: calendar_feed_tokens_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_feed_tokens_user ON public.calendar_feed_tokens USING btree (user_id);


--
-- Name: calendar_file_erasures_due; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_file_erasures_due ON public.calendar_file_erasures USING btree (next_attempt_at);


--
-- Name: calendar_reminder_deliveries_event; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_reminder_deliveries_event ON public.calendar_reminder_deliveries USING btree (event_id);


--
-- Name: calendar_reminder_deliveries_key; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX calendar_reminder_deliveries_key ON public.calendar_reminder_deliveries USING btree (user_id, event_id, occurrence_start, minutes) NULLS NOT DISTINCT;


--
-- Name: calendar_rules_subject; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_rules_subject ON public.calendar_rules USING btree (organisation_id, subject_kind, subject_id);


--
-- Name: calendar_shares_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_shares_user ON public.calendar_shares USING btree (user_id);


--
-- Name: calendars_one_default_per_organisation; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX calendars_one_default_per_organisation ON public.calendars USING btree (organisation_id) WHERE (is_default AND (organisation_id IS NOT NULL));


--
-- Name: calendars_one_default_per_person; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX calendars_one_default_per_person ON public.calendars USING btree (owner_user_id) WHERE (is_default AND (owner_user_id IS NOT NULL));


--
-- Name: calendars_organisation; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendars_organisation ON public.calendars USING btree (organisation_id);


--
-- Name: calendars_owner; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendars_owner ON public.calendars USING btree (owner_user_id);


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
-- Name: role_holdings_account; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX role_holdings_account ON public.role_holdings USING btree (organisation_id, account_id);


--
-- Name: calendar_event_files calendar_event_file_gone; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER calendar_event_file_gone AFTER DELETE ON public.calendar_event_files FOR EACH ROW EXECUTE FUNCTION public.calendar_event_file_gone();


--
-- Name: calendar_attendee_overrides calendar_attendee_overrides_attendee_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_attendee_overrides
    ADD CONSTRAINT calendar_attendee_overrides_attendee_id_fkey FOREIGN KEY (attendee_id) REFERENCES public.calendar_event_attendees(id) ON DELETE CASCADE;


--
-- Name: calendar_event_attendees calendar_event_attendees_event_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_event_attendees
    ADD CONSTRAINT calendar_event_attendees_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.calendar_events(id) ON DELETE CASCADE;


--
-- Name: calendar_event_exceptions calendar_event_exceptions_event_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_event_exceptions
    ADD CONSTRAINT calendar_event_exceptions_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.calendar_events(id) ON DELETE CASCADE;


--
-- Name: calendar_event_files calendar_event_files_event_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_event_files
    ADD CONSTRAINT calendar_event_files_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.calendar_events(id) ON DELETE CASCADE;


--
-- Name: calendar_event_reminders calendar_event_reminders_event_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_event_reminders
    ADD CONSTRAINT calendar_event_reminders_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.calendar_events(id) ON DELETE CASCADE;


--
-- Name: calendar_events calendar_events_calendar_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_events
    ADD CONSTRAINT calendar_events_calendar_id_fkey FOREIGN KEY (calendar_id) REFERENCES public.calendars(id) ON DELETE CASCADE;


--
-- Name: calendar_feed_tokens calendar_feed_tokens_calendar_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_feed_tokens
    ADD CONSTRAINT calendar_feed_tokens_calendar_id_fkey FOREIGN KEY (calendar_id) REFERENCES public.calendars(id) ON DELETE CASCADE;


--
-- Name: calendar_reminder_deliveries calendar_reminder_deliveries_event_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_reminder_deliveries
    ADD CONSTRAINT calendar_reminder_deliveries_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.calendar_events(id) ON DELETE CASCADE;


--
-- Name: calendar_reminder_prefs calendar_reminder_prefs_calendar_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_reminder_prefs
    ADD CONSTRAINT calendar_reminder_prefs_calendar_id_fkey FOREIGN KEY (calendar_id) REFERENCES public.calendars(id) ON DELETE CASCADE;


--
-- Name: calendar_rules calendar_rules_calendar_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_rules
    ADD CONSTRAINT calendar_rules_calendar_id_fkey FOREIGN KEY (calendar_id) REFERENCES public.calendars(id) ON DELETE CASCADE;


--
-- Name: calendar_shares calendar_shares_calendar_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_shares
    ADD CONSTRAINT calendar_shares_calendar_id_fkey FOREIGN KEY (calendar_id) REFERENCES public.calendars(id) ON DELETE CASCADE;


--
-- Name: calendar_visibility calendar_visibility_calendar_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_visibility
    ADD CONSTRAINT calendar_visibility_calendar_id_fkey FOREIGN KEY (calendar_id) REFERENCES public.calendars(id) ON DELETE CASCADE;


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
-- PostgreSQL database dump complete
--


