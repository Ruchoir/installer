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

SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: event_deliveries; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.event_deliveries (
    id uuid NOT NULL,
    event_id uuid NOT NULL,
    kind text NOT NULL,
    subscriber text NOT NULL,
    endpoint text NOT NULL,
    body bytea NOT NULL,
    signature text NOT NULL,
    attempts integer DEFAULT 0 NOT NULL,
    next_attempt_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone DEFAULT (now() + '24:00:00'::interval) NOT NULL,
    delivered_at timestamp with time zone,
    last_error text,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: event_subscriptions; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.event_subscriptions (
    subscriber text NOT NULL,
    kind text NOT NULL,
    endpoint text NOT NULL,
    CONSTRAINT event_subscriptions_endpoint_check CHECK (((endpoint ~~ 'https://%'::text) OR (endpoint ~~ 'http://127.0.0.1:%'::text) OR (endpoint ~~ 'http://localhost:%'::text)))
);


--
-- Name: inbox_entries; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.inbox_entries (
    id uuid NOT NULL,
    account_id uuid NOT NULL,
    product text NOT NULL,
    kind text NOT NULL,
    category text NOT NULL,
    organisation_id uuid,
    actor_id uuid,
    object_id text NOT NULL,
    link text NOT NULL,
    variables jsonb DEFAULT '{}'::jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    read_at timestamp with time zone,
    mail_handled_at timestamp with time zone,
    CONSTRAINT inbox_entries_category_check CHECK ((category = ANY (ARRAY['mention'::text, 'reply'::text, 'direct'::text, 'activity'::text, 'reminder'::text, 'invitation'::text, 'notice'::text]))),
    CONSTRAINT inbox_entries_kind_check CHECK ((kind ~ '^[a-z][a-z0-9_]*\.[a-z][a-z0-9_.]*$'::text)),
    CONSTRAINT inbox_entries_link_check CHECK ((char_length(link) <= 2000)),
    CONSTRAINT inbox_entries_object_id_check CHECK (((char_length(object_id) >= 1) AND (char_length(object_id) <= 200)))
);


--
-- Name: notification_settings; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.notification_settings (
    account_id uuid NOT NULL,
    prefs jsonb NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: organisation_levels; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.organisation_levels (
    account_id uuid NOT NULL,
    organisation_id uuid NOT NULL,
    level text NOT NULL,
    CONSTRAINT organisation_levels_level_check CHECK ((level = ANY (ARRAY['all'::text, 'mentions'::text, 'none'::text])))
);


--
-- Name: organisation_members; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.organisation_members (
    organisation_id uuid NOT NULL,
    account_id uuid NOT NULL
);


--
-- Name: presence_choices; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.presence_choices (
    account_id uuid NOT NULL,
    choice text NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT presence_choices_choice_check CHECK ((choice = ANY (ARRAY['active'::text, 'away'::text, 'dnd'::text, 'invisible'::text])))
);


--
-- Name: push_devices; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.push_devices (
    id uuid NOT NULL,
    account_id uuid NOT NULL,
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
    account_id uuid NOT NULL,
    endpoint text NOT NULL,
    p256dh text NOT NULL,
    auth text NOT NULL,
    user_agent text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    last_success_at timestamp with time zone,
    failures integer DEFAULT 0 NOT NULL,
    CONSTRAINT push_subscriptions_endpoint_check CHECK ((char_length(endpoint) <= 2048))
);


--
-- Name: replica_cursors; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.replica_cursors (
    feed text NOT NULL,
    seq bigint NOT NULL
);


--
-- Name: seaql_migrations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.seaql_migrations (
    version character varying NOT NULL,
    applied_at bigint NOT NULL
);


--
-- Name: event_deliveries event_deliveries_event_id_subscriber_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.event_deliveries
    ADD CONSTRAINT event_deliveries_event_id_subscriber_key UNIQUE (event_id, subscriber);


--
-- Name: event_deliveries event_deliveries_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.event_deliveries
    ADD CONSTRAINT event_deliveries_pkey PRIMARY KEY (id);


--
-- Name: event_subscriptions event_subscriptions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.event_subscriptions
    ADD CONSTRAINT event_subscriptions_pkey PRIMARY KEY (subscriber, kind);


--
-- Name: inbox_entries inbox_entries_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.inbox_entries
    ADD CONSTRAINT inbox_entries_pkey PRIMARY KEY (id);


--
-- Name: notification_settings notification_settings_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.notification_settings
    ADD CONSTRAINT notification_settings_pkey PRIMARY KEY (account_id);


--
-- Name: organisation_levels organisation_levels_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.organisation_levels
    ADD CONSTRAINT organisation_levels_pkey PRIMARY KEY (account_id, organisation_id);


--
-- Name: organisation_members organisation_members_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.organisation_members
    ADD CONSTRAINT organisation_members_pkey PRIMARY KEY (organisation_id, account_id);


--
-- Name: presence_choices presence_choices_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.presence_choices
    ADD CONSTRAINT presence_choices_pkey PRIMARY KEY (account_id);


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
-- Name: replica_cursors replica_cursors_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.replica_cursors
    ADD CONSTRAINT replica_cursors_pkey PRIMARY KEY (feed);


--
-- Name: seaql_migrations seaql_migrations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.seaql_migrations
    ADD CONSTRAINT seaql_migrations_pkey PRIMARY KEY (version);


--
-- Name: event_deliveries_due; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX event_deliveries_due ON public.event_deliveries USING btree (next_attempt_at) WHERE (delivered_at IS NULL);


--
-- Name: inbox_entries_feed; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX inbox_entries_feed ON public.inbox_entries USING btree (account_id, created_at DESC, id DESC);


--
-- Name: inbox_entries_mail_pending; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX inbox_entries_mail_pending ON public.inbox_entries USING btree (created_at) WHERE (mail_handled_at IS NULL);


--
-- Name: inbox_entries_object; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX inbox_entries_object ON public.inbox_entries USING btree (product, object_id);


--
-- Name: inbox_entries_unread; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX inbox_entries_unread ON public.inbox_entries USING btree (account_id) WHERE (read_at IS NULL);


--
-- Name: organisation_members_account; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX organisation_members_account ON public.organisation_members USING btree (account_id);


--
-- Name: push_devices_account; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX push_devices_account ON public.push_devices USING btree (account_id);


--
-- Name: push_subscriptions_account; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX push_subscriptions_account ON public.push_subscriptions USING btree (account_id);


--
-- PostgreSQL database dump complete
--


