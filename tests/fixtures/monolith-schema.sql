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
-- Name: citext; Type: EXTENSION; Schema: -; Owner: -
--

CREATE EXTENSION IF NOT EXISTS citext WITH SCHEMA public;


--
-- Name: EXTENSION citext; Type: COMMENT; Schema: -; Owner: -
--

COMMENT ON EXTENSION citext IS 'data type for case-insensitive character strings';


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
-- Name: permissions_convert_space(uuid); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.permissions_convert_space(p_space uuid) RETURNS void
    LANGUAGE plpgsql
    AS $$
BEGIN
    PERFORM permissions_ensure_system_roles(p_space);
    UPDATE space_members m
       SET standing = d.standing,
           expires_at = CASE WHEN d.standing = 'guest' THEN m.expires_at END,
           expiry_notice_sent_at = CASE WHEN d.standing = 'guest' THEN m.expiry_notice_sent_at END
      FROM (SELECT user_id,
                   CASE role WHEN 'owner' THEN 'owner' WHEN 'guest' THEN 'guest' ELSE 'member' END
                       AS standing
              FROM space_members WHERE space_id = p_space) d
     WHERE m.space_id = p_space AND m.user_id = d.user_id AND m.standing IS DISTINCT FROM d.standing;
    INSERT INTO space_role_assignments (space_id, user_id, role_id)
    SELECT m.space_id, m.user_id, r.id
      FROM space_members m
      JOIN space_roles r ON r.space_id = m.space_id AND r.system_key = 'administrator'
     WHERE m.space_id = p_space AND m.role = 'admin'
    ON CONFLICT DO NOTHING;
    DELETE FROM space_role_assignments a
     USING space_roles r, space_members m
     WHERE a.role_id = r.id AND r.system_key = 'administrator' AND r.space_id = p_space
       AND m.space_id = a.space_id AND m.user_id = a.user_id AND m.role <> 'admin';
    PERFORM permissions_sync_reservation(id) FROM channels WHERE space_id = p_space;
    PERFORM permissions_sync_calendar(id) FROM calendars WHERE space_id = p_space;
END $$;


--
-- Name: permissions_ensure_system_roles(uuid); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.permissions_ensure_system_roles(p_space uuid) RETURNS void
    LANGUAGE plpgsql
    AS $$
BEGIN
    INSERT INTO space_roles (space_id, system_key, name, position, permissions)
    VALUES (p_space, 'administrator', 'Administrator', 100, ARRAY['space.settings', 'members.view', 'members.invite', 'members.manage', 'roles.manage', 'groups.manage', 'audit.view', 'channels.create', 'channels.delete', 'calendars.create', 'dm.start', 'files.trash.purge', 'channel.view', 'message.send', 'message.react', 'message.attach', 'message.mention_all', 'message.pin', 'message.manage', 'channel.members.manage', 'channel.manage', 'files.view', 'files.create', 'files.edit', 'files.manage', 'files.public_link', 'calendar.view', 'calendar.write', 'calendar.manage']::text[]),
           (p_space, 'members', '@members', 0, ARRAY['members.view', 'channels.create', 'dm.start', 'channel.view', 'message.send', 'message.react', 'message.attach', 'message.mention_all', 'message.pin', 'files.view', 'files.create', 'files.edit', 'calendar.view', 'calendar.write']::text[]),
           (p_space, 'guests', '@guests', -1, ARRAY['dm.start', 'message.send', 'message.react', 'message.attach', 'message.mention_all']::text[])
    ON CONFLICT (space_id, system_key) DO NOTHING;
END $$;


--
-- Name: permissions_member_roles(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.permissions_member_roles() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_admin uuid;
BEGIN
    IF NEW.role = 'admin' THEN
        PERFORM permissions_ensure_system_roles(NEW.space_id);
    END IF;
    SELECT id INTO v_admin FROM space_roles
     WHERE space_id = NEW.space_id AND system_key = 'administrator';
    IF NEW.role = 'admin' THEN
        INSERT INTO space_role_assignments (space_id, user_id, role_id)
        VALUES (NEW.space_id, NEW.user_id, v_admin)
        ON CONFLICT DO NOTHING;
    ELSE
        DELETE FROM space_role_assignments WHERE role_id = v_admin AND user_id = NEW.user_id;
    END IF;
    RETURN NULL;
END $$;


--
-- Name: permissions_member_standing(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.permissions_member_standing() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    NEW.standing := CASE NEW.role WHEN 'owner' THEN 'owner' WHEN 'guest' THEN 'guest' ELSE 'member' END;
    IF NEW.standing <> 'guest' THEN
        NEW.expires_at := NULL;
        NEW.expiry_notice_sent_at := NULL;
        NEW.expiry_stage := NULL;
    END IF;
    RETURN NEW;
END $$;


--
-- Name: permissions_on_calendar(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.permissions_on_calendar() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    PERFORM permissions_sync_calendar(NEW.id);
    RETURN NULL;
END $$;


--
-- Name: permissions_on_reservation(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.permissions_on_reservation() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF TG_OP <> 'DELETE' THEN
        PERFORM permissions_sync_reservation(NEW.channel_id);
    END IF;
    IF TG_OP <> 'INSERT' THEN
        PERFORM permissions_sync_reservation(OLD.channel_id);
    END IF;
    RETURN NULL;
END $$;


--
-- Name: permissions_on_space(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.permissions_on_space() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    PERFORM permissions_ensure_system_roles(NEW.id);
    RETURN NULL;
END $$;


--
-- Name: permissions_sync_calendar(uuid); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.permissions_sync_calendar(p_calendar uuid) RETURNS void
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_space uuid;
    v_write text;
BEGIN
    SELECT space_id, write_access INTO v_space, v_write FROM calendars WHERE id = p_calendar;
    IF v_space IS NULL THEN
        RETURN;
    END IF;
    PERFORM permissions_ensure_system_roles(v_space);
    UPDATE permission_overrides o
       SET allow = array_remove(o.allow, 'calendar.write'),
           deny = array_remove(o.deny, 'calendar.write'),
           updated_at = now()
      FROM space_roles r
     WHERE o.calendar_id = p_calendar AND o.role_id = r.id AND r.system_key IS NOT NULL
       AND ('calendar.write' = ANY (o.allow) OR 'calendar.write' = ANY (o.deny));
    IF v_write = 'admins' THEN
        PERFORM permissions_system_rule(v_space, NULL, p_calendar, 'members', 'calendar.write', false);
        PERFORM permissions_system_rule(v_space, NULL, p_calendar, 'administrator', 'calendar.write', true);
    END IF;
    DELETE FROM permission_overrides o USING space_roles r
     WHERE o.calendar_id = p_calendar AND o.role_id = r.id AND r.system_key IS NOT NULL
       AND cardinality(o.allow) = 0 AND cardinality(o.deny) = 0;
END $$;


--
-- Name: permissions_sync_reservation(uuid); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.permissions_sync_reservation(p_channel uuid) RETURNS void
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_space uuid;
    v_roles text[];
BEGIN
    SELECT space_id INTO v_space FROM channels WHERE id = p_channel;
    IF v_space IS NULL THEN
        RETURN;
    END IF;
    PERFORM permissions_ensure_system_roles(v_space);
    UPDATE permission_overrides o
       SET allow = array_remove(o.allow, 'channel.view'),
           deny = array_remove(o.deny, 'channel.view'),
           updated_at = now()
      FROM space_roles r
     WHERE o.channel_id = p_channel AND o.role_id = r.id AND r.system_key IS NOT NULL
       AND ('channel.view' = ANY (o.allow) OR 'channel.view' = ANY (o.deny));
    SELECT coalesce(array_agg(role), '{}') INTO v_roles
      FROM channel_role_access WHERE channel_id = p_channel;
    IF cardinality(v_roles) > 0 THEN
        IF NOT 'member' = ANY (v_roles) THEN
            PERFORM permissions_system_rule(v_space, p_channel, NULL, 'members', 'channel.view', false);
        END IF;
        IF NOT 'guest' = ANY (v_roles) THEN
            PERFORM permissions_system_rule(v_space, p_channel, NULL, 'guests', 'channel.view', false);
        END IF;
        PERFORM permissions_system_rule(v_space, p_channel, NULL, 'administrator', 'channel.view',
                                        'admin' = ANY (v_roles));
    END IF;
    DELETE FROM permission_overrides o USING space_roles r
     WHERE o.channel_id = p_channel AND o.role_id = r.id AND r.system_key IS NOT NULL
       AND cardinality(o.allow) = 0 AND cardinality(o.deny) = 0;
END $$;


--
-- Name: permissions_system_rule(uuid, uuid, uuid, text, text, boolean); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.permissions_system_rule(p_space uuid, p_channel uuid, p_calendar uuid, p_key text, p_permission text, p_allow boolean) RETURNS void
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_role uuid;
    v_allow text[] := CASE WHEN p_allow THEN ARRAY[p_permission] ELSE '{}'::text[] END;
    v_deny text[] := CASE WHEN p_allow THEN '{}'::text[] ELSE ARRAY[p_permission] END;
BEGIN
    SELECT id INTO v_role FROM space_roles WHERE space_id = p_space AND system_key = p_key;
    IF p_channel IS NOT NULL THEN
        INSERT INTO permission_overrides (space_id, channel_id, role_id, allow, deny)
        VALUES (p_space, p_channel, v_role, v_allow, v_deny)
        ON CONFLICT (channel_id, role_id) WHERE channel_id IS NOT NULL AND role_id IS NOT NULL
        DO UPDATE SET
            allow = array_remove(permission_overrides.allow, p_permission) || v_allow,
            deny = array_remove(permission_overrides.deny, p_permission) || v_deny,
            updated_at = now();
    ELSE
        INSERT INTO permission_overrides (space_id, calendar_id, role_id, allow, deny)
        VALUES (p_space, p_calendar, v_role, v_allow, v_deny)
        ON CONFLICT (calendar_id, role_id) WHERE calendar_id IS NOT NULL AND role_id IS NOT NULL
        DO UPDATE SET
            allow = array_remove(permission_overrides.allow, p_permission) || v_allow,
            deny = array_remove(permission_overrides.deny, p_permission) || v_deny,
            updated_at = now();
    END IF;
END $$;


--
-- Name: ruchoir_unaccent(text); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.ruchoir_unaccent(text) RETURNS text
    LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE
    AS $_$ SELECT public.unaccent('public.unaccent'::regdictionary, $1) $_$;


--
-- Name: space_audit_log_append_only(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.space_audit_log_append_only() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF TG_OP = 'UPDATE' THEN
        IF OLD.actor_id IS NOT NULL AND NEW.actor_id IS NULL
           AND NEW.id = OLD.id AND NEW.space_id = OLD.space_id AND NEW.at = OLD.at
           AND NEW.action = OLD.action AND NEW.target_kind = OLD.target_kind
           AND NEW.target_id IS NOT DISTINCT FROM OLD.target_id AND NEW.details = OLD.details THEN
            RETURN NEW;
        END IF;
        RAISE EXCEPTION 'the audit log is append-only';
    END IF;
    IF EXISTS (SELECT 1 FROM spaces WHERE id = OLD.space_id) THEN
        RAISE EXCEPTION 'the audit log is append-only';
    END IF;
    RETURN OLD;
END $$;


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
-- Name: calendar_attendee_overrides; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendar_attendee_overrides (
    attendee_id uuid NOT NULL,
    recurrence_id text NOT NULL,
    status text NOT NULL,
    CONSTRAINT calendar_attendee_overrides_status_check CHECK ((status = ANY (ARRAY['needs_action'::text, 'accepted'::text, 'tentative'::text, 'declined'::text])))
);


--
-- Name: calendar_event_attendees; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendar_event_attendees (
    id uuid NOT NULL,
    event_id uuid NOT NULL,
    user_id uuid,
    email text,
    name text,
    status text DEFAULT 'needs_action'::text NOT NULL,
    token_digest text,
    token_cipher bytea,
    token_nonce bytea,
    invited_by uuid,
    invited_at timestamp with time zone DEFAULT now() NOT NULL,
    responded_at timestamp with time zone,
    CONSTRAINT calendar_event_attendees_email_check CHECK (((email IS NULL) OR ((char_length(email) >= 3) AND (char_length(email) <= 320)))),
    CONSTRAINT calendar_event_attendees_name_check CHECK (((name IS NULL) OR (char_length(name) <= 200))),
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
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    channel_id uuid,
    CONSTRAINT calendar_events_when_check CHECK (((all_day AND (start_date IS NOT NULL) AND (end_date IS NOT NULL) AND (end_date > start_date) AND (start_at IS NULL) AND (end_at IS NULL) AND (tzid IS NULL)) OR ((NOT all_day) AND (start_at IS NOT NULL) AND (end_at IS NOT NULL) AND (end_at >= start_at) AND (tzid IS NOT NULL) AND (start_date IS NULL) AND (end_date IS NULL))))
);


--
-- Name: calendar_feed_tokens; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.calendar_feed_tokens (
    id uuid NOT NULL,
    user_id uuid NOT NULL,
    calendar_id uuid,
    token_hash text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    last_used_at timestamp with time zone,
    revoked_at timestamp with time zone
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
    space_id uuid,
    owner_user_id uuid,
    name text NOT NULL,
    description text,
    color text NOT NULL,
    write_access text DEFAULT 'members'::text NOT NULL,
    is_default boolean DEFAULT false NOT NULL,
    created_by uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    default_reminders integer[] DEFAULT '{}'::integer[] NOT NULL,
    CONSTRAINT calendars_color_check CHECK ((color = ANY (ARRAY['sky'::text, 'mint'::text, 'violet'::text, 'pink'::text, 'peach'::text, 'lime'::text, 'sun'::text, 'accent'::text]))),
    CONSTRAINT calendars_name_check CHECK (((char_length(name) >= 1) AND (char_length(name) <= 200))),
    CONSTRAINT calendars_owner_check CHECK (((space_id IS NULL) <> (owner_user_id IS NULL))),
    CONSTRAINT calendars_write_access_check CHECK ((write_access = ANY (ARRAY['members'::text, 'admins'::text])))
);


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
    joined_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    CONSTRAINT channel_members_role_check CHECK ((role = ANY (ARRAY['owner'::text, 'admin'::text, 'member'::text]))),
    CONSTRAINT ck_channel_members_notification_level CHECK ((notification_level = ANY (ARRAY['default'::text, 'all'::text, 'mentions'::text, 'none'::text])))
);


--
-- Name: channel_pins; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.channel_pins (
    channel_id uuid NOT NULL,
    message_id uuid NOT NULL,
    pinned_by uuid,
    pinned_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);


--
-- Name: channel_role_access; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.channel_role_access (
    channel_id uuid NOT NULL,
    role text NOT NULL,
    CONSTRAINT channel_role_access_role_check CHECK ((role = ANY (ARRAY['owner'::text, 'admin'::text, 'member'::text, 'guest'::text])))
);


--
-- Name: channels; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.channels (
    id uuid NOT NULL,
    space_id uuid NOT NULL,
    name public.citext NOT NULL,
    type text DEFAULT 'public'::text NOT NULL,
    topic text,
    created_by uuid,
    archived_at timestamp with time zone,
    imported_source text,
    external_ref text,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    "position" integer,
    CONSTRAINT channels_type_check CHECK ((type = ANY (ARRAY['public'::text, 'private'::text, 'archived'::text])))
);


--
-- Name: conversations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.conversations (
    id uuid NOT NULL,
    space_id uuid NOT NULL,
    kind text NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    CONSTRAINT conversations_kind_check CHECK ((kind = ANY (ARRAY['channel'::text, 'direct'::text])))
);


--
-- Name: dm_conversations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.dm_conversations (
    id uuid NOT NULL,
    space_id uuid NOT NULL,
    is_group boolean DEFAULT false NOT NULL,
    created_by uuid,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);


--
-- Name: dm_participants; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.dm_participants (
    dm_id uuid NOT NULL,
    user_id uuid NOT NULL,
    added_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    notification_level text DEFAULT 'default'::text NOT NULL,
    muted boolean DEFAULT false NOT NULL,
    hidden boolean DEFAULT false NOT NULL,
    CONSTRAINT ck_dm_participants_notification_level CHECK ((notification_level = ANY (ARRAY['default'::text, 'all'::text, 'mentions'::text, 'none'::text])))
);


--
-- Name: file_links; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.file_links (
    id uuid NOT NULL,
    file_id uuid NOT NULL,
    space_id uuid NOT NULL,
    token text NOT NULL,
    created_by uuid,
    created_at timestamp with time zone NOT NULL,
    expires_at timestamp with time zone,
    password_hash text,
    download_count integer DEFAULT 0 NOT NULL,
    revoked_at timestamp with time zone
);


--
-- Name: file_shares; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.file_shares (
    id uuid NOT NULL,
    file_id uuid NOT NULL,
    shared_by uuid,
    target_user_id uuid,
    target_channel_id uuid,
    permission text DEFAULT 'view'::text NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    expires_at timestamp with time zone,
    CONSTRAINT file_shares_check CHECK (((target_user_id IS NOT NULL) <> (target_channel_id IS NOT NULL))),
    CONSTRAINT file_shares_permission_check CHECK ((permission = ANY (ARRAY['view'::text, 'edit'::text])))
);


--
-- Name: file_stars; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.file_stars (
    user_id uuid NOT NULL,
    file_id uuid NOT NULL,
    created_at timestamp with time zone NOT NULL
);


--
-- Name: file_versions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.file_versions (
    id uuid NOT NULL,
    file_id uuid NOT NULL,
    version_no integer NOT NULL,
    size_bytes bigint DEFAULT 0 NOT NULL,
    content_hash bytea,
    storage_key text,
    mime_type text DEFAULT 'application/octet-stream'::text NOT NULL,
    created_by uuid,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    image_width integer,
    image_height integer,
    thumbnail_key text
);


--
-- Name: files; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.files (
    id uuid NOT NULL,
    space_id uuid NOT NULL,
    owner_id uuid,
    name character varying NOT NULL,
    kind text DEFAULT 'file'::text NOT NULL,
    parent_folder_id uuid,
    current_version_id uuid,
    size_bytes bigint DEFAULT 0 NOT NULL,
    imported_source text,
    external_ref text,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    deleted_at timestamp with time zone,
    conversation_id uuid,
    system_key text,
    deleted_by uuid,
    trashed boolean DEFAULT false NOT NULL,
    purged_at timestamp with time zone,
    CONSTRAINT files_kind_check CHECK ((kind = ANY (ARRAY['file'::text, 'file-text'::text, 'file-spreadsheet'::text, 'image'::text, 'folder'::text])))
);


--
-- Name: import_jobs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.import_jobs (
    id uuid NOT NULL,
    space_id uuid,
    created_by uuid,
    source text NOT NULL,
    status text DEFAULT 'pending'::text NOT NULL,
    options text DEFAULT '{}'::text NOT NULL,
    archive_file_id uuid,
    manifest text,
    channels_total integer DEFAULT 0 NOT NULL,
    channels_done integer DEFAULT 0 NOT NULL,
    messages_total integer DEFAULT 0 NOT NULL,
    messages_done integer DEFAULT 0 NOT NULL,
    files_total integer DEFAULT 0 NOT NULL,
    files_done integer DEFAULT 0 NOT NULL,
    accounts_total integer DEFAULT 0 NOT NULL,
    accounts_done integer DEFAULT 0 NOT NULL,
    error text,
    started_at timestamp with time zone,
    finished_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    calendars_total integer DEFAULT 0 NOT NULL,
    calendars_done integer DEFAULT 0 NOT NULL,
    events_total integer DEFAULT 0 NOT NULL,
    events_done integer DEFAULT 0 NOT NULL,
    summary jsonb,
    attempt integer DEFAULT 0 NOT NULL,
    CONSTRAINT import_jobs_source_check CHECK ((source = ANY (ARRAY['nextcloud'::text, 'mattermost'::text, 'slack'::text, 'teams'::text, 'synthetic'::text]))),
    CONSTRAINT import_jobs_status_check CHECK ((status = ANY (ARRAY['pending'::text, 'analyzing'::text, 'ready'::text, 'running'::text, 'cancelling'::text, 'cancelled'::text, 'completed'::text, 'failed'::text])))
);


--
-- Name: import_mappings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.import_mappings (
    id uuid NOT NULL,
    job_id uuid NOT NULL,
    space_id uuid,
    source text NOT NULL,
    kind text NOT NULL,
    external_ref text NOT NULL,
    internal_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    owner_id uuid,
    CONSTRAINT import_mappings_kind_check CHECK ((kind = ANY (ARRAY['space'::text, 'user'::text, 'channel'::text, 'message'::text, 'file'::text, 'calendar'::text, 'event'::text, 'group'::text, 'role'::text]))),
    CONSTRAINT import_mappings_source_check CHECK ((source = ANY (ARRAY['nextcloud'::text, 'mattermost'::text, 'slack'::text, 'teams'::text, 'synthetic'::text])))
);


--
-- Name: instance_events; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.instance_events (
    id uuid NOT NULL,
    kind text NOT NULL,
    occurred_at timestamp with time zone DEFAULT now() NOT NULL,
    actor_id uuid,
    detail text DEFAULT '{}'::text NOT NULL,
    CONSTRAINT instance_events_kind_check CHECK ((kind = ANY (ARRAY['backup_taken'::text, 'instance_replaced'::text])))
);


--
-- Name: instance_settings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.instance_settings (
    id boolean DEFAULT true NOT NULL,
    show_instance_admins boolean DEFAULT true NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    web_push_enabled boolean DEFAULT true NOT NULL,
    vapid_public_key text,
    vapid_private_key bytea,
    vapid_private_nonce bytea,
    CONSTRAINT instance_settings_id_check CHECK (id)
);


--
-- Name: licence_instance; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.licence_instance (
    id smallint NOT NULL,
    instance_id uuid NOT NULL,
    highest_seen timestamp with time zone NOT NULL,
    CONSTRAINT licence_instance_id_check CHECK ((id = 1))
);


--
-- Name: licence_kill_notices; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.licence_kill_notices (
    serial uuid NOT NULL,
    notice text NOT NULL,
    received_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: message_attachments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.message_attachments (
    message_id uuid NOT NULL,
    file_id uuid NOT NULL,
    file_version_id uuid,
    "position" integer DEFAULT 0 NOT NULL,
    alt_text text
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
    image_file_id uuid,
    fetched_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    expires_at timestamp with time zone,
    color text,
    image_key text,
    image_width integer,
    image_height integer
);


--
-- Name: message_mentions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.message_mentions (
    message_id uuid NOT NULL,
    mentioned_user_id uuid NOT NULL,
    mention_type text DEFAULT 'user'::text NOT NULL,
    CONSTRAINT message_mentions_mention_type_check CHECK ((mention_type = ANY (ARRAY['user'::text, 'channel'::text, 'here'::text, 'group'::text])))
);


--
-- Name: message_reactions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.message_reactions (
    message_id uuid NOT NULL,
    user_id uuid NOT NULL,
    emoji text NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
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
    reply_count integer DEFAULT 0 NOT NULL,
    imported_source text,
    external_ref text,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    edited_at timestamp with time zone,
    deleted_at timestamp with time zone,
    search_vector tsvector GENERATED ALWAYS AS (to_tsvector('public.fr_unaccent'::regconfig, body)) STORED,
    event_id uuid,
    also_in_channel boolean DEFAULT false NOT NULL,
    CONSTRAINT messages_kind_check CHECK ((kind = ANY (ARRAY['message'::text, 'system'::text])))
);


--
-- Name: notifications; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.notifications (
    id uuid NOT NULL,
    user_id uuid NOT NULL,
    kind text NOT NULL,
    conversation_id uuid,
    message_id uuid,
    actor_id uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    read_at timestamp with time zone,
    email_handled_at timestamp with time zone,
    event_id uuid,
    occurrence_start timestamp with time zone,
    payload jsonb,
    CONSTRAINT notifications_kind_check CHECK ((kind = ANY (ARRAY['mention'::text, 'broadcast'::text, 'reply'::text, 'dm'::text, 'message'::text, 'reaction'::text, 'edited'::text, 'calendar_reminder'::text, 'calendar_invitation'::text, 'calendar_update'::text, 'calendar_cancel'::text, 'calendar_declined'::text, 'calendar_shared'::text, 'guest_expiry_soon'::text, 'guest_access_ending'::text]))),
    CONSTRAINT notifications_subject_check CHECK ((((kind = 'calendar_reminder'::text) AND (event_id IS NOT NULL) AND (occurrence_start IS NOT NULL) AND (conversation_id IS NULL) AND (message_id IS NULL)) OR ((kind = ANY (ARRAY['calendar_invitation'::text, 'calendar_update'::text, 'calendar_declined'::text])) AND (event_id IS NOT NULL) AND (payload IS NOT NULL) AND (conversation_id IS NULL) AND (message_id IS NULL)) OR ((kind = ANY (ARRAY['calendar_cancel'::text, 'calendar_shared'::text])) AND (payload IS NOT NULL) AND (conversation_id IS NULL) AND (message_id IS NULL)) OR ((kind = ANY (ARRAY['guest_expiry_soon'::text, 'guest_access_ending'::text])) AND (payload IS NOT NULL) AND (event_id IS NULL) AND (conversation_id IS NULL) AND (message_id IS NULL)) OR ((kind !~~ 'calendar\_%'::text) AND (kind !~~ 'guest\_%'::text) AND (conversation_id IS NOT NULL) AND (message_id IS NOT NULL))))
);


--
-- Name: permission_overrides; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.permission_overrides (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    space_id uuid NOT NULL,
    channel_id uuid,
    file_id uuid,
    calendar_id uuid,
    role_id uuid,
    user_id uuid,
    allow text[] DEFAULT '{}'::text[] NOT NULL,
    deny text[] DEFAULT '{}'::text[] NOT NULL,
    created_by uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    group_id uuid,
    audience_channel_id uuid,
    set_by uuid,
    set_at timestamp with time zone,
    CONSTRAINT permission_overrides_audience_files_only CHECK (((audience_channel_id IS NULL) OR (file_id IS NOT NULL))),
    CONSTRAINT permission_overrides_one_resource CHECK ((num_nonnulls(channel_id, file_id, calendar_id) = 1)),
    CONSTRAINT permission_overrides_one_target CHECK ((num_nonnulls(role_id, user_id, group_id, audience_channel_id) = 1))
);


--
-- Name: push_devices; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.push_devices (
    id uuid NOT NULL,
    user_id uuid NOT NULL,
    platform text NOT NULL,
    token text NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    last_seen_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT push_devices_platform_check CHECK ((platform = ANY (ARRAY['ios'::text, 'android'::text])))
);


--
-- Name: push_subscriptions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.push_subscriptions (
    id uuid NOT NULL,
    user_id uuid NOT NULL,
    endpoint text NOT NULL,
    p256dh text NOT NULL,
    auth text NOT NULL,
    user_agent text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    last_success_at timestamp with time zone,
    failures integer DEFAULT 0 NOT NULL
);


--
-- Name: read_cursors; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.read_cursors (
    conversation_id uuid NOT NULL,
    user_id uuid NOT NULL,
    last_read_message_id uuid,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);


--
-- Name: recovery_codes; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.recovery_codes (
    id uuid NOT NULL,
    user_id uuid NOT NULL,
    code_hash bytea NOT NULL,
    used_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);


--
-- Name: seaql_migrations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.seaql_migrations (
    version character varying NOT NULL,
    applied_at bigint NOT NULL
);


--
-- Name: space_audit_log; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.space_audit_log (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    space_id uuid NOT NULL,
    at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    actor_id uuid,
    action text NOT NULL,
    target_kind text NOT NULL,
    target_id uuid,
    details jsonb DEFAULT '{}'::jsonb NOT NULL,
    CONSTRAINT space_audit_log_target_kind_check CHECK ((target_kind = ANY (ARRAY['member'::text, 'role'::text, 'channel'::text, 'calendar'::text, 'space'::text, 'licence'::text, 'group'::text, 'file'::text])))
);


--
-- Name: space_group_members; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.space_group_members (
    group_id uuid NOT NULL,
    space_id uuid NOT NULL,
    user_id uuid NOT NULL,
    is_manager boolean DEFAULT false NOT NULL,
    added_by uuid,
    added_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: space_group_roles; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.space_group_roles (
    group_id uuid NOT NULL,
    space_id uuid NOT NULL,
    role_id uuid NOT NULL
);


--
-- Name: space_groups; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.space_groups (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    space_id uuid NOT NULL,
    name public.citext NOT NULL,
    description text,
    source text DEFAULT 'manual'::text NOT NULL,
    external_id text,
    created_by uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    imported_from text,
    CONSTRAINT space_groups_description_check CHECK ((char_length(description) <= 280)),
    CONSTRAINT space_groups_name_check CHECK ((((char_length((name)::text) >= 1) AND (char_length((name)::text) <= 100)) AND (btrim((name)::text) <> ''::text))),
    CONSTRAINT space_groups_source_check CHECK ((source = ANY (ARRAY['manual'::text, 'directory'::text])))
);


--
-- Name: space_invitations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.space_invitations (
    id uuid NOT NULL,
    space_id uuid NOT NULL,
    token_hash text NOT NULL,
    email public.citext,
    role text DEFAULT 'member'::text NOT NULL,
    created_by uuid,
    max_uses integer,
    uses integer DEFAULT 0 NOT NULL,
    expires_at timestamp with time zone,
    revoked_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    channel_id uuid,
    guest_expires_at timestamp with time zone,
    CONSTRAINT space_invitations_guest_expires_check CHECK (((guest_expires_at IS NULL) OR (role = 'guest'::text))),
    CONSTRAINT space_invitations_role_check CHECK ((role = ANY (ARRAY['admin'::text, 'member'::text, 'guest'::text])))
);


--
-- Name: space_licences; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.space_licences (
    space_id uuid NOT NULL,
    licence text NOT NULL,
    licence_id uuid NOT NULL,
    organisation text NOT NULL,
    seats integer NOT NULL,
    valid_until timestamp with time zone NOT NULL,
    installed_by uuid,
    installed_at timestamp with time zone DEFAULT now() NOT NULL,
    reminded_days integer,
    next_licence text,
    status_token text,
    status_token_issued_at timestamp with time zone,
    last_check_in_at timestamp with time zone,
    last_attempt_at timestamp with time zone,
    CONSTRAINT space_licences_reminded_days_check CHECK ((reminded_days = ANY (ARRAY[30, 7, 1]))),
    CONSTRAINT space_licences_seats_check CHECK ((seats > 0))
);


--
-- Name: space_members; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.space_members (
    space_id uuid NOT NULL,
    user_id uuid NOT NULL,
    role text DEFAULT 'member'::text NOT NULL,
    invited_by uuid,
    joined_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    standing text NOT NULL,
    expires_at timestamp with time zone,
    expiry_notice_sent_at timestamp with time zone,
    expiry_stage smallint,
    imported_from text,
    CONSTRAINT space_members_expiry_check CHECK (((expires_at IS NULL) OR (standing = 'guest'::text))),
    CONSTRAINT space_members_expiry_stage_check CHECK (((expiry_stage IS NULL) OR (expiry_stage = ANY (ARRAY[0, 1, 7])))),
    CONSTRAINT space_members_expiry_stage_guest_check CHECK (((expiry_stage IS NULL) OR (standing = 'guest'::text))),
    CONSTRAINT space_members_role_check CHECK ((role = ANY (ARRAY['owner'::text, 'admin'::text, 'member'::text, 'guest'::text]))),
    CONSTRAINT space_members_standing_check CHECK ((standing = ANY (ARRAY['owner'::text, 'member'::text, 'guest'::text])))
);


--
-- Name: space_notification_prefs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.space_notification_prefs (
    user_id uuid NOT NULL,
    space_id uuid NOT NULL,
    level text NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT space_notification_prefs_level_check CHECK ((level = ANY (ARRAY['all'::text, 'mentions'::text, 'none'::text])))
);


--
-- Name: space_role_assignments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.space_role_assignments (
    space_id uuid NOT NULL,
    user_id uuid NOT NULL,
    role_id uuid NOT NULL
);


--
-- Name: space_role_holdings; Type: VIEW; Schema: public; Owner: -
--

CREATE VIEW public.space_role_holdings AS
 SELECT space_role_assignments.space_id,
    space_role_assignments.user_id,
    space_role_assignments.role_id,
    NULL::uuid AS group_id
   FROM public.space_role_assignments
UNION ALL
 SELECT m.space_id,
    m.user_id,
    r.role_id,
    m.group_id
   FROM (public.space_group_members m
     JOIN public.space_group_roles r ON ((r.group_id = m.group_id)));


--
-- Name: space_roles; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.space_roles (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    space_id uuid NOT NULL,
    system_key text,
    name public.citext NOT NULL,
    colour text,
    "position" integer DEFAULT 0 NOT NULL,
    permissions text[] DEFAULT '{}'::text[] NOT NULL,
    created_by uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT space_roles_colour_check CHECK ((colour = ANY (ARRAY['sky'::text, 'mint'::text, 'violet'::text, 'pink'::text, 'peach'::text, 'lime'::text, 'sun'::text, 'accent'::text]))),
    CONSTRAINT space_roles_name_check CHECK (((char_length((name)::text) >= 1) AND (char_length((name)::text) <= 100))),
    CONSTRAINT space_roles_system_key_check CHECK ((system_key = ANY (ARRAY['administrator'::text, 'members'::text, 'guests'::text])))
);


--
-- Name: space_slugs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.space_slugs (
    slug text NOT NULL,
    space_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);


--
-- Name: spaces; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.spaces (
    id uuid NOT NULL,
    name character varying NOT NULL,
    slug public.citext NOT NULL,
    created_by uuid,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    icon_key text,
    default_channel_id uuid
);


--
-- Name: totp_secrets; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.totp_secrets (
    user_id uuid NOT NULL,
    secret_ciphertext bytea NOT NULL,
    secret_nonce bytea NOT NULL,
    confirmed_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);


--
-- Name: user_preferences; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.user_preferences (
    user_id uuid NOT NULL,
    theme text,
    font text,
    text_size text,
    emoji_pack text,
    emoji_animated boolean,
    notifications text,
    ui_state text,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);


--
-- Name: user_saved_messages; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.user_saved_messages (
    user_id uuid NOT NULL,
    message_id uuid NOT NULL,
    saved_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL
);


--
-- Name: users; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.users (
    id uuid NOT NULL,
    email public.citext NOT NULL,
    display_name character varying NOT NULL,
    password_hash text,
    status character varying DEFAULT 'pending'::character varying NOT NULL,
    mfa_enforced boolean DEFAULT false NOT NULL,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    updated_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    title text,
    pronouns text,
    timezone text,
    bio text,
    is_bot boolean DEFAULT false NOT NULL,
    manual_presence text,
    avatar_key text,
    is_instance_admin boolean DEFAULT false NOT NULL,
    locale text,
    CONSTRAINT ck_users_locale CHECK (((locale IS NULL) OR (locale = ANY (ARRAY['fr'::text, 'en'::text, 'es'::text, 'de'::text, 'it'::text, 'pl'::text])))),
    CONSTRAINT ck_users_manual_presence CHECK (((manual_presence IS NULL) OR (manual_presence = ANY (ARRAY['active'::text, 'away'::text, 'dnd'::text, 'invisible'::text]))))
);


--
-- Name: webauthn_credentials; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.webauthn_credentials (
    id uuid NOT NULL,
    user_id uuid NOT NULL,
    credential_id bytea NOT NULL,
    public_key bytea NOT NULL,
    sign_count bigint DEFAULT 0 NOT NULL,
    transports character varying,
    label character varying,
    created_at timestamp with time zone DEFAULT CURRENT_TIMESTAMP NOT NULL,
    last_used_at timestamp with time zone
);


--
-- Name: calendar_attendee_overrides calendar_attendee_overrides_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_attendee_overrides
    ADD CONSTRAINT calendar_attendee_overrides_pkey PRIMARY KEY (attendee_id, recurrence_id);


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
-- Name: calendar_feed_tokens calendar_feed_tokens_token_hash_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_feed_tokens
    ADD CONSTRAINT calendar_feed_tokens_token_hash_key UNIQUE (token_hash);


--
-- Name: calendar_reminder_prefs calendar_reminder_prefs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_reminder_prefs
    ADD CONSTRAINT calendar_reminder_prefs_pkey PRIMARY KEY (user_id, calendar_id);


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
-- Name: calendars calendars_id_space_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendars
    ADD CONSTRAINT calendars_id_space_key UNIQUE (id, space_id);


--
-- Name: calendars calendars_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendars
    ADD CONSTRAINT calendars_pkey PRIMARY KEY (id);


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
-- Name: channel_role_access channel_role_access_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.channel_role_access
    ADD CONSTRAINT channel_role_access_pkey PRIMARY KEY (channel_id, role);


--
-- Name: channels channels_id_space_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.channels
    ADD CONSTRAINT channels_id_space_key UNIQUE (id, space_id);


--
-- Name: channels channels_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.channels
    ADD CONSTRAINT channels_pkey PRIMARY KEY (id);


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
-- Name: file_links file_links_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_links
    ADD CONSTRAINT file_links_pkey PRIMARY KEY (id);


--
-- Name: file_links file_links_token_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_links
    ADD CONSTRAINT file_links_token_key UNIQUE (token);


--
-- Name: file_shares file_shares_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_shares
    ADD CONSTRAINT file_shares_pkey PRIMARY KEY (id);


--
-- Name: file_stars file_stars_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_stars
    ADD CONSTRAINT file_stars_pkey PRIMARY KEY (user_id, file_id);


--
-- Name: file_versions file_versions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_versions
    ADD CONSTRAINT file_versions_pkey PRIMARY KEY (id);


--
-- Name: files files_id_space_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.files
    ADD CONSTRAINT files_id_space_key UNIQUE (id, space_id);


--
-- Name: files files_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.files
    ADD CONSTRAINT files_pkey PRIMARY KEY (id);


--
-- Name: import_jobs import_jobs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.import_jobs
    ADD CONSTRAINT import_jobs_pkey PRIMARY KEY (id);


--
-- Name: import_mappings import_mappings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.import_mappings
    ADD CONSTRAINT import_mappings_pkey PRIMARY KEY (id);


--
-- Name: instance_events instance_events_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.instance_events
    ADD CONSTRAINT instance_events_pkey PRIMARY KEY (id);


--
-- Name: instance_settings instance_settings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.instance_settings
    ADD CONSTRAINT instance_settings_pkey PRIMARY KEY (id);


--
-- Name: licence_instance licence_instance_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.licence_instance
    ADD CONSTRAINT licence_instance_pkey PRIMARY KEY (id);


--
-- Name: licence_kill_notices licence_kill_notices_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.licence_kill_notices
    ADD CONSTRAINT licence_kill_notices_pkey PRIMARY KEY (serial);


--
-- Name: message_attachments message_attachments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_attachments
    ADD CONSTRAINT message_attachments_pkey PRIMARY KEY (message_id, file_id);


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
-- Name: notifications notifications_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_pkey PRIMARY KEY (id);


--
-- Name: permission_overrides permission_overrides_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.permission_overrides
    ADD CONSTRAINT permission_overrides_pkey PRIMARY KEY (id);


--
-- Name: push_devices push_devices_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.push_devices
    ADD CONSTRAINT push_devices_pkey PRIMARY KEY (id);


--
-- Name: push_devices push_devices_token_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.push_devices
    ADD CONSTRAINT push_devices_token_key UNIQUE (token);


--
-- Name: push_subscriptions push_subscriptions_endpoint_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.push_subscriptions
    ADD CONSTRAINT push_subscriptions_endpoint_key UNIQUE (endpoint);


--
-- Name: push_subscriptions push_subscriptions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.push_subscriptions
    ADD CONSTRAINT push_subscriptions_pkey PRIMARY KEY (id);


--
-- Name: read_cursors read_cursors_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.read_cursors
    ADD CONSTRAINT read_cursors_pkey PRIMARY KEY (conversation_id, user_id);


--
-- Name: recovery_codes recovery_codes_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recovery_codes
    ADD CONSTRAINT recovery_codes_pkey PRIMARY KEY (id);


--
-- Name: seaql_migrations seaql_migrations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.seaql_migrations
    ADD CONSTRAINT seaql_migrations_pkey PRIMARY KEY (version);


--
-- Name: space_audit_log space_audit_log_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_audit_log
    ADD CONSTRAINT space_audit_log_pkey PRIMARY KEY (id);


--
-- Name: space_group_members space_group_members_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_group_members
    ADD CONSTRAINT space_group_members_pkey PRIMARY KEY (group_id, user_id);


--
-- Name: space_group_roles space_group_roles_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_group_roles
    ADD CONSTRAINT space_group_roles_pkey PRIMARY KEY (group_id, role_id);


--
-- Name: space_groups space_groups_id_space_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_groups
    ADD CONSTRAINT space_groups_id_space_id_key UNIQUE (id, space_id);


--
-- Name: space_groups space_groups_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_groups
    ADD CONSTRAINT space_groups_pkey PRIMARY KEY (id);


--
-- Name: space_groups space_groups_space_id_name_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_groups
    ADD CONSTRAINT space_groups_space_id_name_key UNIQUE (space_id, name);


--
-- Name: space_invitations space_invitations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_invitations
    ADD CONSTRAINT space_invitations_pkey PRIMARY KEY (id);


--
-- Name: space_invitations space_invitations_token_hash_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_invitations
    ADD CONSTRAINT space_invitations_token_hash_key UNIQUE (token_hash);


--
-- Name: space_licences space_licences_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_licences
    ADD CONSTRAINT space_licences_pkey PRIMARY KEY (space_id);


--
-- Name: space_members space_members_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_members
    ADD CONSTRAINT space_members_pkey PRIMARY KEY (space_id, user_id);


--
-- Name: space_notification_prefs space_notification_prefs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_notification_prefs
    ADD CONSTRAINT space_notification_prefs_pkey PRIMARY KEY (user_id, space_id);


--
-- Name: space_role_assignments space_role_assignments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_role_assignments
    ADD CONSTRAINT space_role_assignments_pkey PRIMARY KEY (role_id, user_id);


--
-- Name: space_roles space_roles_id_space_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_roles
    ADD CONSTRAINT space_roles_id_space_key UNIQUE (id, space_id);


--
-- Name: space_roles space_roles_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_roles
    ADD CONSTRAINT space_roles_pkey PRIMARY KEY (id);


--
-- Name: space_roles space_roles_system_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_roles
    ADD CONSTRAINT space_roles_system_key UNIQUE (space_id, system_key);


--
-- Name: space_slugs space_slugs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_slugs
    ADD CONSTRAINT space_slugs_pkey PRIMARY KEY (slug);


--
-- Name: spaces spaces_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.spaces
    ADD CONSTRAINT spaces_pkey PRIMARY KEY (id);


--
-- Name: totp_secrets totp_secrets_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.totp_secrets
    ADD CONSTRAINT totp_secrets_pkey PRIMARY KEY (user_id);


--
-- Name: user_preferences user_preferences_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_preferences
    ADD CONSTRAINT user_preferences_pkey PRIMARY KEY (user_id);


--
-- Name: user_saved_messages user_saved_messages_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_saved_messages
    ADD CONSTRAINT user_saved_messages_pkey PRIMARY KEY (user_id, message_id);


--
-- Name: users users_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.users
    ADD CONSTRAINT users_pkey PRIMARY KEY (id);


--
-- Name: webauthn_credentials webauthn_credentials_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webauthn_credentials
    ADD CONSTRAINT webauthn_credentials_pkey PRIMARY KEY (id);


--
-- Name: calendar_event_attendees_email; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX calendar_event_attendees_email ON public.calendar_event_attendees USING btree (event_id, lower(email)) WHERE (email IS NOT NULL);


--
-- Name: calendar_event_attendees_person; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_event_attendees_person ON public.calendar_event_attendees USING btree (user_id) WHERE (user_id IS NOT NULL);


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
-- Name: calendar_events_channel; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_events_channel ON public.calendar_events USING btree (channel_id) WHERE (channel_id IS NOT NULL);


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
-- Name: calendar_reminder_deliveries_key; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX calendar_reminder_deliveries_key ON public.calendar_reminder_deliveries USING btree (user_id, event_id, occurrence_start, minutes) NULLS NOT DISTINCT;


--
-- Name: calendar_shares_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendar_shares_user ON public.calendar_shares USING btree (user_id);


--
-- Name: calendars_one_default_per_person; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX calendars_one_default_per_person ON public.calendars USING btree (owner_user_id) WHERE (is_default AND (owner_user_id IS NOT NULL));


--
-- Name: calendars_one_default_per_space; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX calendars_one_default_per_space ON public.calendars USING btree (space_id) WHERE (is_default AND (space_id IS NOT NULL));


--
-- Name: calendars_owner; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendars_owner ON public.calendars USING btree (owner_user_id);


--
-- Name: calendars_space; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX calendars_space ON public.calendars USING btree (space_id);


--
-- Name: channels_space_position_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX channels_space_position_idx ON public.channels USING btree (space_id, "position", created_at, id);


--
-- Name: files_name_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX files_name_trgm ON public.files USING gin (public.ruchoir_unaccent((name)::text) public.gin_trgm_ops);


--
-- Name: idx_channels_space_name_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_channels_space_name_unique ON public.channels USING btree (space_id, name);


--
-- Name: idx_file_links_file; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_file_links_file ON public.file_links USING btree (file_id);


--
-- Name: idx_file_versions_content_hash; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_file_versions_content_hash ON public.file_versions USING btree (content_hash);


--
-- Name: idx_file_versions_file_no_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_file_versions_file_no_unique ON public.file_versions USING btree (file_id, version_no);


--
-- Name: idx_files_conversation; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_files_conversation ON public.files USING btree (conversation_id);


--
-- Name: idx_files_space_parent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_files_space_parent ON public.files USING btree (space_id, parent_folder_id);


--
-- Name: idx_files_system_key_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_files_system_key_unique ON public.files USING btree (space_id, system_key);


--
-- Name: idx_files_trash; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_files_trash ON public.files USING btree (space_id, deleted_at) WHERE trashed;


--
-- Name: idx_message_link_previews_message; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_message_link_previews_message ON public.message_link_previews USING btree (message_id);


--
-- Name: idx_message_reactions_user_recent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_message_reactions_user_recent ON public.message_reactions USING btree (user_id, created_at DESC);


--
-- Name: idx_messages_conversation_created; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_messages_conversation_created ON public.messages USING btree (conversation_id, created_at);


--
-- Name: idx_messages_parent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_messages_parent ON public.messages USING btree (parent_message_id);


--
-- Name: idx_notifications_email_pending; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_notifications_email_pending ON public.notifications USING btree (created_at) WHERE (email_handled_at IS NULL);


--
-- Name: idx_push_devices_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_push_devices_user ON public.push_devices USING btree (user_id);


--
-- Name: idx_push_subscriptions_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_push_subscriptions_user ON public.push_subscriptions USING btree (user_id);


--
-- Name: idx_recovery_codes_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_recovery_codes_user ON public.recovery_codes USING btree (user_id);


--
-- Name: idx_space_invitations_space; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_space_invitations_space ON public.space_invitations USING btree (space_id, created_at);


--
-- Name: idx_space_slugs_space; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_space_slugs_space ON public.space_slugs USING btree (space_id);


--
-- Name: idx_spaces_slug_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_spaces_slug_unique ON public.spaces USING btree (slug);


--
-- Name: idx_users_email_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_users_email_unique ON public.users USING btree (email);


--
-- Name: idx_webauthn_credential_id_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_webauthn_credential_id_unique ON public.webauthn_credentials USING btree (credential_id);


--
-- Name: import_jobs_space_created_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX import_jobs_space_created_idx ON public.import_jobs USING btree (space_id, created_at DESC);


--
-- Name: import_mappings_identity_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX import_mappings_identity_idx ON public.import_mappings USING btree (space_id, source, kind, external_ref) WHERE (space_id IS NOT NULL);


--
-- Name: import_mappings_instance_identity_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX import_mappings_instance_identity_idx ON public.import_mappings USING btree (source, kind, external_ref, owner_id) NULLS NOT DISTINCT WHERE (space_id IS NULL);


--
-- Name: import_mappings_job_kind_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX import_mappings_job_kind_idx ON public.import_mappings USING btree (job_id, kind);


--
-- Name: instance_events_kind_time_idx; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX instance_events_kind_time_idx ON public.instance_events USING btree (kind, occurred_at DESC);


--
-- Name: messages_body_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX messages_body_trgm ON public.messages USING gin (public.ruchoir_unaccent(body) public.gin_trgm_ops);


--
-- Name: messages_event; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX messages_event ON public.messages USING btree (event_id) WHERE (event_id IS NOT NULL);


--
-- Name: messages_search_vector_gin; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX messages_search_vector_gin ON public.messages USING gin (search_vector);


--
-- Name: notifications_event; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX notifications_event ON public.notifications USING btree (event_id) WHERE (event_id IS NOT NULL);


--
-- Name: notifications_user_created; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX notifications_user_created ON public.notifications USING btree (user_id, created_at DESC);


--
-- Name: notifications_user_unread; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX notifications_user_unread ON public.notifications USING btree (user_id) WHERE (read_at IS NULL);


--
-- Name: permission_overrides_audience; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX permission_overrides_audience ON public.permission_overrides USING btree (audience_channel_id) WHERE (audience_channel_id IS NOT NULL);


--
-- Name: permission_overrides_calendar_group; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX permission_overrides_calendar_group ON public.permission_overrides USING btree (calendar_id, group_id) WHERE ((calendar_id IS NOT NULL) AND (group_id IS NOT NULL));


--
-- Name: permission_overrides_calendar_role; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX permission_overrides_calendar_role ON public.permission_overrides USING btree (calendar_id, role_id) WHERE ((calendar_id IS NOT NULL) AND (role_id IS NOT NULL));


--
-- Name: permission_overrides_calendar_user; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX permission_overrides_calendar_user ON public.permission_overrides USING btree (calendar_id, user_id) WHERE ((calendar_id IS NOT NULL) AND (user_id IS NOT NULL));


--
-- Name: permission_overrides_channel_group; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX permission_overrides_channel_group ON public.permission_overrides USING btree (channel_id, group_id) WHERE ((channel_id IS NOT NULL) AND (group_id IS NOT NULL));


--
-- Name: permission_overrides_channel_role; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX permission_overrides_channel_role ON public.permission_overrides USING btree (channel_id, role_id) WHERE ((channel_id IS NOT NULL) AND (role_id IS NOT NULL));


--
-- Name: permission_overrides_channel_user; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX permission_overrides_channel_user ON public.permission_overrides USING btree (channel_id, user_id) WHERE ((channel_id IS NOT NULL) AND (user_id IS NOT NULL));


--
-- Name: permission_overrides_file_audience; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX permission_overrides_file_audience ON public.permission_overrides USING btree (file_id, audience_channel_id) WHERE ((file_id IS NOT NULL) AND (audience_channel_id IS NOT NULL));


--
-- Name: permission_overrides_file_group; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX permission_overrides_file_group ON public.permission_overrides USING btree (file_id, group_id) WHERE ((file_id IS NOT NULL) AND (group_id IS NOT NULL));


--
-- Name: permission_overrides_file_role; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX permission_overrides_file_role ON public.permission_overrides USING btree (file_id, role_id) WHERE ((file_id IS NOT NULL) AND (role_id IS NOT NULL));


--
-- Name: permission_overrides_file_user; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX permission_overrides_file_user ON public.permission_overrides USING btree (file_id, user_id) WHERE ((file_id IS NOT NULL) AND (user_id IS NOT NULL));


--
-- Name: permission_overrides_group; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX permission_overrides_group ON public.permission_overrides USING btree (group_id) WHERE (group_id IS NOT NULL);


--
-- Name: permission_overrides_space; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX permission_overrides_space ON public.permission_overrides USING btree (space_id);


--
-- Name: space_audit_log_space_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX space_audit_log_space_at ON public.space_audit_log USING btree (space_id, at DESC, id DESC);


--
-- Name: space_group_members_member; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX space_group_members_member ON public.space_group_members USING btree (space_id, user_id);


--
-- Name: space_group_roles_role; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX space_group_roles_role ON public.space_group_roles USING btree (role_id);


--
-- Name: space_groups_external_id_key; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX space_groups_external_id_key ON public.space_groups USING btree (space_id, external_id) WHERE (external_id IS NOT NULL);


--
-- Name: space_members_expires_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX space_members_expires_at ON public.space_members USING btree (expires_at) WHERE (expires_at IS NOT NULL);


--
-- Name: space_role_assignments_member; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX space_role_assignments_member ON public.space_role_assignments USING btree (space_id, user_id);


--
-- Name: space_roles_name_key; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX space_roles_name_key ON public.space_roles USING btree (space_id, name) WHERE (system_key IS NULL);


--
-- Name: calendars calendars_write_rules; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER calendars_write_rules AFTER INSERT OR UPDATE OF write_access ON public.calendars FOR EACH ROW EXECUTE FUNCTION public.permissions_on_calendar();


--
-- Name: channel_role_access channel_role_access_rules; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER channel_role_access_rules AFTER INSERT OR DELETE OR UPDATE ON public.channel_role_access FOR EACH ROW EXECUTE FUNCTION public.permissions_on_reservation();


--
-- Name: space_audit_log space_audit_log_append_only; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER space_audit_log_append_only BEFORE DELETE OR UPDATE ON public.space_audit_log FOR EACH ROW EXECUTE FUNCTION public.space_audit_log_append_only();


--
-- Name: space_members space_members_roles; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER space_members_roles AFTER INSERT OR UPDATE OF role ON public.space_members FOR EACH ROW EXECUTE FUNCTION public.permissions_member_roles();


--
-- Name: space_members space_members_standing; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER space_members_standing BEFORE INSERT OR UPDATE OF role ON public.space_members FOR EACH ROW EXECUTE FUNCTION public.permissions_member_standing();


--
-- Name: spaces spaces_system_roles; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER spaces_system_roles AFTER INSERT ON public.spaces FOR EACH ROW EXECUTE FUNCTION public.permissions_on_space();


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
-- Name: calendar_event_attendees calendar_event_attendees_invited_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_event_attendees
    ADD CONSTRAINT calendar_event_attendees_invited_by_fkey FOREIGN KEY (invited_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: calendar_event_attendees calendar_event_attendees_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_event_attendees
    ADD CONSTRAINT calendar_event_attendees_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: calendar_event_exceptions calendar_event_exceptions_event_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_event_exceptions
    ADD CONSTRAINT calendar_event_exceptions_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.calendar_events(id) ON DELETE CASCADE;


--
-- Name: calendar_event_files calendar_event_files_added_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_event_files
    ADD CONSTRAINT calendar_event_files_added_by_fkey FOREIGN KEY (added_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: calendar_event_files calendar_event_files_event_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_event_files
    ADD CONSTRAINT calendar_event_files_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.calendar_events(id) ON DELETE CASCADE;


--
-- Name: calendar_event_files calendar_event_files_file_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_event_files
    ADD CONSTRAINT calendar_event_files_file_id_fkey FOREIGN KEY (file_id) REFERENCES public.files(id) ON DELETE CASCADE;


--
-- Name: calendar_event_reminders calendar_event_reminders_event_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_event_reminders
    ADD CONSTRAINT calendar_event_reminders_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.calendar_events(id) ON DELETE CASCADE;


--
-- Name: calendar_event_reminders calendar_event_reminders_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_event_reminders
    ADD CONSTRAINT calendar_event_reminders_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: calendar_events calendar_events_calendar_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_events
    ADD CONSTRAINT calendar_events_calendar_id_fkey FOREIGN KEY (calendar_id) REFERENCES public.calendars(id) ON DELETE CASCADE;


--
-- Name: calendar_events calendar_events_channel_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_events
    ADD CONSTRAINT calendar_events_channel_id_fkey FOREIGN KEY (channel_id) REFERENCES public.channels(id) ON DELETE SET NULL;


--
-- Name: calendar_events calendar_events_created_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_events
    ADD CONSTRAINT calendar_events_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: calendar_events calendar_events_updated_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_events
    ADD CONSTRAINT calendar_events_updated_by_fkey FOREIGN KEY (updated_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: calendar_feed_tokens calendar_feed_tokens_calendar_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_feed_tokens
    ADD CONSTRAINT calendar_feed_tokens_calendar_id_fkey FOREIGN KEY (calendar_id) REFERENCES public.calendars(id) ON DELETE CASCADE;


--
-- Name: calendar_feed_tokens calendar_feed_tokens_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_feed_tokens
    ADD CONSTRAINT calendar_feed_tokens_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: calendar_reminder_deliveries calendar_reminder_deliveries_event_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_reminder_deliveries
    ADD CONSTRAINT calendar_reminder_deliveries_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.calendar_events(id) ON DELETE CASCADE;


--
-- Name: calendar_reminder_deliveries calendar_reminder_deliveries_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_reminder_deliveries
    ADD CONSTRAINT calendar_reminder_deliveries_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: calendar_reminder_prefs calendar_reminder_prefs_calendar_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_reminder_prefs
    ADD CONSTRAINT calendar_reminder_prefs_calendar_id_fkey FOREIGN KEY (calendar_id) REFERENCES public.calendars(id) ON DELETE CASCADE;


--
-- Name: calendar_reminder_prefs calendar_reminder_prefs_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_reminder_prefs
    ADD CONSTRAINT calendar_reminder_prefs_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: calendar_shares calendar_shares_calendar_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_shares
    ADD CONSTRAINT calendar_shares_calendar_id_fkey FOREIGN KEY (calendar_id) REFERENCES public.calendars(id) ON DELETE CASCADE;


--
-- Name: calendar_shares calendar_shares_created_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_shares
    ADD CONSTRAINT calendar_shares_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: calendar_shares calendar_shares_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_shares
    ADD CONSTRAINT calendar_shares_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: calendar_visibility calendar_visibility_calendar_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_visibility
    ADD CONSTRAINT calendar_visibility_calendar_id_fkey FOREIGN KEY (calendar_id) REFERENCES public.calendars(id) ON DELETE CASCADE;


--
-- Name: calendar_visibility calendar_visibility_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendar_visibility
    ADD CONSTRAINT calendar_visibility_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: calendars calendars_created_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendars
    ADD CONSTRAINT calendars_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: calendars calendars_owner_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendars
    ADD CONSTRAINT calendars_owner_user_id_fkey FOREIGN KEY (owner_user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: calendars calendars_space_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.calendars
    ADD CONSTRAINT calendars_space_id_fkey FOREIGN KEY (space_id) REFERENCES public.spaces(id) ON DELETE CASCADE;


--
-- Name: channel_role_access channel_role_access_channel_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.channel_role_access
    ADD CONSTRAINT channel_role_access_channel_id_fkey FOREIGN KEY (channel_id) REFERENCES public.channels(id) ON DELETE CASCADE;


--
-- Name: file_links file_links_file_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_links
    ADD CONSTRAINT file_links_file_id_fkey FOREIGN KEY (file_id) REFERENCES public.files(id) ON DELETE CASCADE;


--
-- Name: file_stars file_stars_file_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_stars
    ADD CONSTRAINT file_stars_file_id_fkey FOREIGN KEY (file_id) REFERENCES public.files(id) ON DELETE CASCADE;


--
-- Name: file_stars file_stars_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_stars
    ADD CONSTRAINT file_stars_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: channel_members fk_channel_members_channel; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.channel_members
    ADD CONSTRAINT fk_channel_members_channel FOREIGN KEY (channel_id) REFERENCES public.channels(id) ON DELETE CASCADE;


--
-- Name: channel_members fk_channel_members_user; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.channel_members
    ADD CONSTRAINT fk_channel_members_user FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: channel_pins fk_channel_pins_channel; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.channel_pins
    ADD CONSTRAINT fk_channel_pins_channel FOREIGN KEY (channel_id) REFERENCES public.channels(id) ON DELETE CASCADE;


--
-- Name: channel_pins fk_channel_pins_message; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.channel_pins
    ADD CONSTRAINT fk_channel_pins_message FOREIGN KEY (message_id) REFERENCES public.messages(id) ON DELETE CASCADE;


--
-- Name: channel_pins fk_channel_pins_pinned_by; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.channel_pins
    ADD CONSTRAINT fk_channel_pins_pinned_by FOREIGN KEY (pinned_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: channels fk_channels_conversation; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.channels
    ADD CONSTRAINT fk_channels_conversation FOREIGN KEY (id) REFERENCES public.conversations(id) ON DELETE CASCADE;


--
-- Name: channels fk_channels_created_by; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.channels
    ADD CONSTRAINT fk_channels_created_by FOREIGN KEY (created_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: channels fk_channels_space; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.channels
    ADD CONSTRAINT fk_channels_space FOREIGN KEY (space_id) REFERENCES public.spaces(id) ON DELETE CASCADE;


--
-- Name: conversations fk_conversations_space; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.conversations
    ADD CONSTRAINT fk_conversations_space FOREIGN KEY (space_id) REFERENCES public.spaces(id) ON DELETE CASCADE;


--
-- Name: dm_conversations fk_dm_conversations_conversation; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.dm_conversations
    ADD CONSTRAINT fk_dm_conversations_conversation FOREIGN KEY (id) REFERENCES public.conversations(id) ON DELETE CASCADE;


--
-- Name: dm_conversations fk_dm_conversations_created_by; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.dm_conversations
    ADD CONSTRAINT fk_dm_conversations_created_by FOREIGN KEY (created_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: dm_conversations fk_dm_conversations_space; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.dm_conversations
    ADD CONSTRAINT fk_dm_conversations_space FOREIGN KEY (space_id) REFERENCES public.spaces(id) ON DELETE CASCADE;


--
-- Name: dm_participants fk_dm_participants_dm; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.dm_participants
    ADD CONSTRAINT fk_dm_participants_dm FOREIGN KEY (dm_id) REFERENCES public.dm_conversations(id) ON DELETE CASCADE;


--
-- Name: dm_participants fk_dm_participants_user; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.dm_participants
    ADD CONSTRAINT fk_dm_participants_user FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: file_shares fk_file_shares_file; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_shares
    ADD CONSTRAINT fk_file_shares_file FOREIGN KEY (file_id) REFERENCES public.files(id) ON DELETE CASCADE;


--
-- Name: file_shares fk_file_shares_shared_by; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_shares
    ADD CONSTRAINT fk_file_shares_shared_by FOREIGN KEY (shared_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: file_shares fk_file_shares_target_channel; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_shares
    ADD CONSTRAINT fk_file_shares_target_channel FOREIGN KEY (target_channel_id) REFERENCES public.conversations(id) ON DELETE CASCADE;


--
-- Name: file_shares fk_file_shares_target_user; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_shares
    ADD CONSTRAINT fk_file_shares_target_user FOREIGN KEY (target_user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: file_versions fk_file_versions_created_by; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_versions
    ADD CONSTRAINT fk_file_versions_created_by FOREIGN KEY (created_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: file_versions fk_file_versions_file; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_versions
    ADD CONSTRAINT fk_file_versions_file FOREIGN KEY (file_id) REFERENCES public.files(id) ON DELETE CASCADE;


--
-- Name: files fk_files_conversation; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.files
    ADD CONSTRAINT fk_files_conversation FOREIGN KEY (conversation_id) REFERENCES public.conversations(id) ON DELETE CASCADE;


--
-- Name: files fk_files_owner; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.files
    ADD CONSTRAINT fk_files_owner FOREIGN KEY (owner_id) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: files fk_files_parent_folder; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.files
    ADD CONSTRAINT fk_files_parent_folder FOREIGN KEY (parent_folder_id) REFERENCES public.files(id) ON DELETE SET NULL;


--
-- Name: files fk_files_space; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.files
    ADD CONSTRAINT fk_files_space FOREIGN KEY (space_id) REFERENCES public.spaces(id) ON DELETE CASCADE;


--
-- Name: message_attachments fk_message_attachments_file; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_attachments
    ADD CONSTRAINT fk_message_attachments_file FOREIGN KEY (file_id) REFERENCES public.files(id) ON DELETE CASCADE;


--
-- Name: message_attachments fk_message_attachments_message; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_attachments
    ADD CONSTRAINT fk_message_attachments_message FOREIGN KEY (message_id) REFERENCES public.messages(id) ON DELETE CASCADE;


--
-- Name: message_attachments fk_message_attachments_version; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_attachments
    ADD CONSTRAINT fk_message_attachments_version FOREIGN KEY (file_version_id) REFERENCES public.file_versions(id) ON DELETE SET NULL;


--
-- Name: message_link_previews fk_message_link_previews_image; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_link_previews
    ADD CONSTRAINT fk_message_link_previews_image FOREIGN KEY (image_file_id) REFERENCES public.files(id) ON DELETE SET NULL;


--
-- Name: message_link_previews fk_message_link_previews_message; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_link_previews
    ADD CONSTRAINT fk_message_link_previews_message FOREIGN KEY (message_id) REFERENCES public.messages(id) ON DELETE CASCADE;


--
-- Name: message_mentions fk_message_mentions_message; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_mentions
    ADD CONSTRAINT fk_message_mentions_message FOREIGN KEY (message_id) REFERENCES public.messages(id) ON DELETE CASCADE;


--
-- Name: message_mentions fk_message_mentions_user; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_mentions
    ADD CONSTRAINT fk_message_mentions_user FOREIGN KEY (mentioned_user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: message_reactions fk_message_reactions_message; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_reactions
    ADD CONSTRAINT fk_message_reactions_message FOREIGN KEY (message_id) REFERENCES public.messages(id) ON DELETE CASCADE;


--
-- Name: message_reactions fk_message_reactions_user; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.message_reactions
    ADD CONSTRAINT fk_message_reactions_user FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: messages fk_messages_author; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.messages
    ADD CONSTRAINT fk_messages_author FOREIGN KEY (author_id) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: messages fk_messages_conversation; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.messages
    ADD CONSTRAINT fk_messages_conversation FOREIGN KEY (conversation_id) REFERENCES public.conversations(id) ON DELETE CASCADE;


--
-- Name: messages fk_messages_parent; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.messages
    ADD CONSTRAINT fk_messages_parent FOREIGN KEY (parent_message_id) REFERENCES public.messages(id) ON DELETE CASCADE;


--
-- Name: read_cursors fk_read_cursors_conversation; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.read_cursors
    ADD CONSTRAINT fk_read_cursors_conversation FOREIGN KEY (conversation_id) REFERENCES public.conversations(id) ON DELETE CASCADE;


--
-- Name: read_cursors fk_read_cursors_last_read; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.read_cursors
    ADD CONSTRAINT fk_read_cursors_last_read FOREIGN KEY (last_read_message_id) REFERENCES public.messages(id) ON DELETE SET NULL;


--
-- Name: read_cursors fk_read_cursors_user; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.read_cursors
    ADD CONSTRAINT fk_read_cursors_user FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: recovery_codes fk_recovery_codes_user; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recovery_codes
    ADD CONSTRAINT fk_recovery_codes_user FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: space_invitations fk_space_invitations_created_by; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_invitations
    ADD CONSTRAINT fk_space_invitations_created_by FOREIGN KEY (created_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: space_invitations fk_space_invitations_space; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_invitations
    ADD CONSTRAINT fk_space_invitations_space FOREIGN KEY (space_id) REFERENCES public.spaces(id) ON DELETE CASCADE;


--
-- Name: space_members fk_space_members_invited_by; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_members
    ADD CONSTRAINT fk_space_members_invited_by FOREIGN KEY (invited_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: space_members fk_space_members_space; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_members
    ADD CONSTRAINT fk_space_members_space FOREIGN KEY (space_id) REFERENCES public.spaces(id) ON DELETE CASCADE;


--
-- Name: space_members fk_space_members_user; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_members
    ADD CONSTRAINT fk_space_members_user FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: space_slugs fk_space_slugs_space; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_slugs
    ADD CONSTRAINT fk_space_slugs_space FOREIGN KEY (space_id) REFERENCES public.spaces(id) ON DELETE CASCADE;


--
-- Name: spaces fk_spaces_created_by; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.spaces
    ADD CONSTRAINT fk_spaces_created_by FOREIGN KEY (created_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: totp_secrets fk_totp_secrets_user; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.totp_secrets
    ADD CONSTRAINT fk_totp_secrets_user FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: user_preferences fk_user_preferences_user; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_preferences
    ADD CONSTRAINT fk_user_preferences_user FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: user_saved_messages fk_user_saved_messages_message; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_saved_messages
    ADD CONSTRAINT fk_user_saved_messages_message FOREIGN KEY (message_id) REFERENCES public.messages(id) ON DELETE CASCADE;


--
-- Name: user_saved_messages fk_user_saved_messages_user; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.user_saved_messages
    ADD CONSTRAINT fk_user_saved_messages_user FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: webauthn_credentials fk_webauthn_credentials_user; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webauthn_credentials
    ADD CONSTRAINT fk_webauthn_credentials_user FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: import_jobs import_jobs_archive_file_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.import_jobs
    ADD CONSTRAINT import_jobs_archive_file_id_fkey FOREIGN KEY (archive_file_id) REFERENCES public.files(id) ON DELETE SET NULL;


--
-- Name: import_jobs import_jobs_created_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.import_jobs
    ADD CONSTRAINT import_jobs_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: import_jobs import_jobs_space_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.import_jobs
    ADD CONSTRAINT import_jobs_space_id_fkey FOREIGN KEY (space_id) REFERENCES public.spaces(id) ON DELETE CASCADE;


--
-- Name: import_mappings import_mappings_job_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.import_mappings
    ADD CONSTRAINT import_mappings_job_id_fkey FOREIGN KEY (job_id) REFERENCES public.import_jobs(id) ON DELETE CASCADE;


--
-- Name: import_mappings import_mappings_space_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.import_mappings
    ADD CONSTRAINT import_mappings_space_id_fkey FOREIGN KEY (space_id) REFERENCES public.spaces(id) ON DELETE CASCADE;


--
-- Name: instance_events instance_events_actor_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.instance_events
    ADD CONSTRAINT instance_events_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: messages messages_event_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.messages
    ADD CONSTRAINT messages_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.calendar_events(id) ON DELETE SET NULL;


--
-- Name: notifications notifications_actor_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: notifications notifications_conversation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_conversation_id_fkey FOREIGN KEY (conversation_id) REFERENCES public.conversations(id) ON DELETE CASCADE;


--
-- Name: notifications notifications_event_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_event_id_fkey FOREIGN KEY (event_id) REFERENCES public.calendar_events(id) ON DELETE CASCADE;


--
-- Name: notifications notifications_message_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_message_id_fkey FOREIGN KEY (message_id) REFERENCES public.messages(id) ON DELETE CASCADE;


--
-- Name: notifications notifications_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notifications
    ADD CONSTRAINT notifications_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: permission_overrides permission_overrides_audience_channel_id_space_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.permission_overrides
    ADD CONSTRAINT permission_overrides_audience_channel_id_space_id_fkey FOREIGN KEY (audience_channel_id, space_id) REFERENCES public.channels(id, space_id) ON DELETE CASCADE;


--
-- Name: permission_overrides permission_overrides_calendar_id_space_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.permission_overrides
    ADD CONSTRAINT permission_overrides_calendar_id_space_id_fkey FOREIGN KEY (calendar_id, space_id) REFERENCES public.calendars(id, space_id) ON DELETE CASCADE;


--
-- Name: permission_overrides permission_overrides_channel_id_space_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.permission_overrides
    ADD CONSTRAINT permission_overrides_channel_id_space_id_fkey FOREIGN KEY (channel_id, space_id) REFERENCES public.channels(id, space_id) ON DELETE CASCADE;


--
-- Name: permission_overrides permission_overrides_created_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.permission_overrides
    ADD CONSTRAINT permission_overrides_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: permission_overrides permission_overrides_file_id_space_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.permission_overrides
    ADD CONSTRAINT permission_overrides_file_id_space_id_fkey FOREIGN KEY (file_id, space_id) REFERENCES public.files(id, space_id) ON DELETE CASCADE;


--
-- Name: permission_overrides permission_overrides_group_id_space_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.permission_overrides
    ADD CONSTRAINT permission_overrides_group_id_space_id_fkey FOREIGN KEY (group_id, space_id) REFERENCES public.space_groups(id, space_id);


--
-- Name: permission_overrides permission_overrides_role_id_space_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.permission_overrides
    ADD CONSTRAINT permission_overrides_role_id_space_id_fkey FOREIGN KEY (role_id, space_id) REFERENCES public.space_roles(id, space_id);


--
-- Name: permission_overrides permission_overrides_set_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.permission_overrides
    ADD CONSTRAINT permission_overrides_set_by_fkey FOREIGN KEY (set_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: permission_overrides permission_overrides_space_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.permission_overrides
    ADD CONSTRAINT permission_overrides_space_id_fkey FOREIGN KEY (space_id) REFERENCES public.spaces(id) ON DELETE CASCADE;


--
-- Name: permission_overrides permission_overrides_space_id_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.permission_overrides
    ADD CONSTRAINT permission_overrides_space_id_user_id_fkey FOREIGN KEY (space_id, user_id) REFERENCES public.space_members(space_id, user_id) ON DELETE CASCADE;


--
-- Name: push_devices push_devices_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.push_devices
    ADD CONSTRAINT push_devices_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: push_subscriptions push_subscriptions_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.push_subscriptions
    ADD CONSTRAINT push_subscriptions_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: space_audit_log space_audit_log_actor_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_audit_log
    ADD CONSTRAINT space_audit_log_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: space_audit_log space_audit_log_space_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_audit_log
    ADD CONSTRAINT space_audit_log_space_id_fkey FOREIGN KEY (space_id) REFERENCES public.spaces(id) ON DELETE CASCADE;


--
-- Name: space_group_members space_group_members_added_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_group_members
    ADD CONSTRAINT space_group_members_added_by_fkey FOREIGN KEY (added_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: space_group_members space_group_members_group_id_space_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_group_members
    ADD CONSTRAINT space_group_members_group_id_space_id_fkey FOREIGN KEY (group_id, space_id) REFERENCES public.space_groups(id, space_id) ON DELETE CASCADE;


--
-- Name: space_group_members space_group_members_space_id_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_group_members
    ADD CONSTRAINT space_group_members_space_id_user_id_fkey FOREIGN KEY (space_id, user_id) REFERENCES public.space_members(space_id, user_id) ON DELETE CASCADE;


--
-- Name: space_group_roles space_group_roles_group_id_space_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_group_roles
    ADD CONSTRAINT space_group_roles_group_id_space_id_fkey FOREIGN KEY (group_id, space_id) REFERENCES public.space_groups(id, space_id) ON DELETE CASCADE;


--
-- Name: space_group_roles space_group_roles_role_id_space_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_group_roles
    ADD CONSTRAINT space_group_roles_role_id_space_id_fkey FOREIGN KEY (role_id, space_id) REFERENCES public.space_roles(id, space_id) ON DELETE CASCADE;


--
-- Name: space_groups space_groups_created_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_groups
    ADD CONSTRAINT space_groups_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: space_groups space_groups_space_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_groups
    ADD CONSTRAINT space_groups_space_id_fkey FOREIGN KEY (space_id) REFERENCES public.spaces(id) ON DELETE CASCADE;


--
-- Name: space_invitations space_invitations_channel_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_invitations
    ADD CONSTRAINT space_invitations_channel_id_fkey FOREIGN KEY (channel_id) REFERENCES public.channels(id) ON DELETE CASCADE;


--
-- Name: space_licences space_licences_installed_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_licences
    ADD CONSTRAINT space_licences_installed_by_fkey FOREIGN KEY (installed_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: space_licences space_licences_space_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_licences
    ADD CONSTRAINT space_licences_space_id_fkey FOREIGN KEY (space_id) REFERENCES public.spaces(id) ON DELETE CASCADE;


--
-- Name: space_notification_prefs space_notification_prefs_space_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_notification_prefs
    ADD CONSTRAINT space_notification_prefs_space_id_fkey FOREIGN KEY (space_id) REFERENCES public.spaces(id) ON DELETE CASCADE;


--
-- Name: space_notification_prefs space_notification_prefs_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_notification_prefs
    ADD CONSTRAINT space_notification_prefs_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: space_role_assignments space_role_assignments_role_id_space_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_role_assignments
    ADD CONSTRAINT space_role_assignments_role_id_space_id_fkey FOREIGN KEY (role_id, space_id) REFERENCES public.space_roles(id, space_id) ON DELETE CASCADE;


--
-- Name: space_role_assignments space_role_assignments_space_id_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_role_assignments
    ADD CONSTRAINT space_role_assignments_space_id_user_id_fkey FOREIGN KEY (space_id, user_id) REFERENCES public.space_members(space_id, user_id) ON DELETE CASCADE;


--
-- Name: space_roles space_roles_created_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_roles
    ADD CONSTRAINT space_roles_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: space_roles space_roles_space_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.space_roles
    ADD CONSTRAINT space_roles_space_id_fkey FOREIGN KEY (space_id) REFERENCES public.spaces(id) ON DELETE CASCADE;


--
-- PostgreSQL database dump complete
--


