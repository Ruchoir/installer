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
-- Name: organisation_journal_append_only(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.organisation_journal_append_only() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    IF TG_OP = 'UPDATE' THEN
        IF OLD.actor_id IS NOT NULL AND NEW.actor_id IS NULL
           AND NEW.id = OLD.id AND NEW.organisation_id = OLD.organisation_id AND NEW.at = OLD.at
           AND NEW.action = OLD.action AND NEW.target_kind = OLD.target_kind
           AND NEW.target_id IS NOT DISTINCT FROM OLD.target_id AND NEW.details = OLD.details THEN
            RETURN NEW;
        END IF;
        RAISE EXCEPTION 'the organisation journal is append-only';
    END IF;
    IF EXISTS (SELECT 1 FROM organisations WHERE id = OLD.organisation_id) THEN
        RAISE EXCEPTION 'the organisation journal is append-only';
    END IF;
    RETURN OLD;
END $$;


--
-- Name: organisations_system_roles(); Type: FUNCTION; Schema: public; Owner: -
--

CREATE FUNCTION public.organisations_system_roles() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    INSERT INTO roles (organisation_id, system_key, name, position, permissions)
    VALUES (NEW.id, 'administrator', 'Administrator', 100, ARRAY['organisation.settings', 'members.view', 'members.invite', 'members.manage', 'groups.manage', 'roles.assign', 'journal.view']::text[]),
           (NEW.id, 'members', '@members', 0, ARRAY['members.view']::text[]),
           (NEW.id, 'guests', '@guests', -1, '{}'::text[])
    ON CONFLICT (organisation_id, system_key) DO NOTHING;
    RETURN NEW;
END $$;


SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: changes; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.changes (
    seq bigint NOT NULL,
    id uuid NOT NULL,
    kind text NOT NULL,
    organisation_id uuid,
    body bytea NOT NULL,
    signature text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    sent_at timestamp with time zone,
    attempts integer DEFAULT 0 NOT NULL,
    next_attempt_at timestamp with time zone DEFAULT now() NOT NULL,
    last_error text
);


--
-- Name: changes_seq_seq; Type: SEQUENCE; Schema: public; Owner: -
--

CREATE SEQUENCE public.changes_seq_seq
    START WITH 1
    INCREMENT BY 1
    NO MINVALUE
    NO MAXVALUE
    CACHE 1;


--
-- Name: changes_seq_seq; Type: SEQUENCE OWNED BY; Schema: public; Owner: -
--

ALTER SEQUENCE public.changes_seq_seq OWNED BY public.changes.seq;


--
-- Name: client_keys; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.client_keys (
    client_id text NOT NULL,
    kid text NOT NULL,
    purpose text NOT NULL,
    jwk jsonb NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    disabled_at timestamp with time zone,
    CONSTRAINT client_keys_purpose_check CHECK ((purpose = ANY (ARRAY['client_auth'::text, 'events'::text])))
);


--
-- Name: group_members; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.group_members (
    group_id uuid NOT NULL,
    organisation_id uuid NOT NULL,
    user_id uuid NOT NULL,
    is_manager boolean DEFAULT false NOT NULL,
    added_by uuid,
    added_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: group_roles; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.group_roles (
    group_id uuid NOT NULL,
    organisation_id uuid NOT NULL,
    role_id uuid NOT NULL
);


--
-- Name: groups; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.groups (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    organisation_id uuid NOT NULL,
    name public.citext NOT NULL,
    description text,
    source text DEFAULT 'manual'::text NOT NULL,
    external_id text,
    created_by uuid,
    imported_from text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT groups_description_check CHECK ((char_length(description) <= 280)),
    CONSTRAINT groups_name_check CHECK ((((char_length((name)::text) >= 1) AND (char_length((name)::text) <= 100)) AND (btrim((name)::text) <> ''::text))),
    CONSTRAINT groups_source_check CHECK ((source = ANY (ARRAY['manual'::text, 'directory'::text])))
);


--
-- Name: invitations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.invitations (
    id uuid NOT NULL,
    organisation_id uuid NOT NULL,
    token_hash text NOT NULL,
    email public.citext,
    standing text DEFAULT 'member'::text NOT NULL,
    role_id uuid,
    created_by uuid,
    max_uses integer,
    uses integer DEFAULT 0 NOT NULL,
    expires_at timestamp with time zone,
    revoked_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    guest_expires_at timestamp with time zone,
    context_client text,
    context jsonb,
    CONSTRAINT invitations_context_has_its_client CHECK (((context_client IS NULL) = (context IS NULL))),
    CONSTRAINT invitations_guest_end_is_a_guests CHECK (((guest_expires_at IS NULL) OR (standing = 'guest'::text))),
    CONSTRAINT invitations_standing_check CHECK ((standing = ANY (ARRAY['member'::text, 'guest'::text])))
);


--
-- Name: logout_deliveries; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.logout_deliveries (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    client_id text NOT NULL,
    sub text NOT NULL,
    sid text,
    attempts integer DEFAULT 0 NOT NULL,
    next_attempt_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone DEFAULT (now() + '01:00:00'::interval) NOT NULL,
    delivered_at timestamp with time zone,
    last_error text,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: memberships; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.memberships (
    organisation_id uuid NOT NULL,
    user_id uuid NOT NULL,
    standing text NOT NULL,
    invited_by uuid,
    joined_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone,
    expiry_notice_sent_at timestamp with time zone,
    expiry_stage smallint,
    imported_from text,
    CONSTRAINT memberships_expiry_is_a_guests CHECK (((expires_at IS NULL) OR (standing = 'guest'::text))),
    CONSTRAINT memberships_expiry_stage_check CHECK ((expiry_stage = ANY (ARRAY[0, 1, 7]))),
    CONSTRAINT memberships_stage_is_a_guests CHECK (((expiry_stage IS NULL) OR (standing = 'guest'::text))),
    CONSTRAINT memberships_standing_check CHECK ((standing = ANY (ARRAY['owner'::text, 'member'::text, 'guest'::text])))
);


--
-- Name: oauth_clients; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.oauth_clients (
    client_id text NOT NULL,
    name text NOT NULL,
    source text NOT NULL,
    first_party boolean DEFAULT false NOT NULL,
    subject_type text DEFAULT 'pairwise'::text NOT NULL,
    token_endpoint_auth_method text NOT NULL,
    client_secret_digest text,
    resource text,
    redirect_uris text[] DEFAULT '{}'::text[] NOT NULL,
    post_logout_redirect_uris text[] DEFAULT '{}'::text[] NOT NULL,
    backchannel_logout_uri text,
    grant_types text[] DEFAULT '{}'::text[] NOT NULL,
    scopes text[] DEFAULT '{}'::text[] NOT NULL,
    resource_scopes text[] DEFAULT '{}'::text[] NOT NULL,
    token_exchange jsonb DEFAULT '[]'::jsonb NOT NULL,
    disabled_at timestamp with time zone,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT oauth_clients_client_id_check CHECK ((client_id ~ '^[a-z0-9][a-z0-9._-]{0,63}$'::text)),
    CONSTRAINT oauth_clients_name_check CHECK (((char_length(name) >= 1) AND (char_length(name) <= 100))),
    CONSTRAINT oauth_clients_secret_for_basic CHECK (((token_endpoint_auth_method = 'client_secret_basic'::text) = (client_secret_digest IS NOT NULL))),
    CONSTRAINT oauth_clients_source_check CHECK ((source = ANY (ARRAY['registry'::text, 'admin'::text]))),
    CONSTRAINT oauth_clients_subject_type_check CHECK ((subject_type = ANY (ARRAY['public'::text, 'pairwise'::text]))),
    CONSTRAINT oauth_clients_token_endpoint_auth_method_check CHECK ((token_endpoint_auth_method = ANY (ARRAY['private_key_jwt'::text, 'client_secret_basic'::text, 'none'::text])))
);


--
-- Name: oauth_consents; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.oauth_consents (
    user_id uuid NOT NULL,
    client_id text NOT NULL,
    scopes text[] NOT NULL,
    organisations uuid[] DEFAULT '{}'::uuid[] NOT NULL,
    granted_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: oauth_grants; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.oauth_grants (
    id uuid NOT NULL,
    user_id uuid NOT NULL,
    client_id text NOT NULL,
    sid text,
    scopes text[] NOT NULL,
    resources text[] NOT NULL,
    organisations uuid[],
    auth_time timestamp with time zone NOT NULL,
    acr text NOT NULL,
    amr text[] NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    last_used_at timestamp with time zone DEFAULT now() NOT NULL,
    expires_at timestamp with time zone NOT NULL,
    revoked_at timestamp with time zone,
    revoked_reason text,
    CONSTRAINT oauth_grants_revoked_reason_check CHECK ((revoked_reason = ANY (ARRAY['reused'::text, 'code_reused'::text, 'session_ended'::text, 'revoked'::text, 'client_disabled'::text])))
);


--
-- Name: oauth_refresh_tokens; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.oauth_refresh_tokens (
    digest text NOT NULL,
    grant_id uuid NOT NULL,
    generation integer NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    spent_at timestamp with time zone,
    CONSTRAINT oauth_refresh_tokens_generation_check CHECK ((generation >= 1))
);


--
-- Name: organisation_journal; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.organisation_journal (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    organisation_id uuid NOT NULL,
    at timestamp with time zone DEFAULT clock_timestamp() NOT NULL,
    actor_id uuid,
    action text NOT NULL,
    target_kind text NOT NULL,
    target_id uuid,
    details jsonb DEFAULT '{}'::jsonb NOT NULL,
    CONSTRAINT organisation_journal_target_kind_check CHECK ((target_kind = ANY (ARRAY['organisation'::text, 'member'::text, 'role'::text, 'group'::text, 'invitation'::text])))
);


--
-- Name: organisation_slugs; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.organisation_slugs (
    slug text NOT NULL,
    organisation_id uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: organisations; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.organisations (
    id uuid NOT NULL,
    name text NOT NULL,
    slug public.citext NOT NULL,
    created_by uuid,
    icon_key text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: pairwise_subjects; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.pairwise_subjects (
    sector text NOT NULL,
    user_id uuid NOT NULL,
    subject uuid NOT NULL,
    created_at timestamp with time zone DEFAULT now() NOT NULL
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
-- Name: role_assignments; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.role_assignments (
    organisation_id uuid NOT NULL,
    user_id uuid NOT NULL,
    role_id uuid NOT NULL
);


--
-- Name: role_holdings; Type: VIEW; Schema: public; Owner: -
--

CREATE VIEW public.role_holdings AS
 SELECT role_assignments.organisation_id,
    role_assignments.user_id,
    role_assignments.role_id,
    NULL::uuid AS group_id
   FROM public.role_assignments
UNION ALL
 SELECT m.organisation_id,
    m.user_id,
    r.role_id,
    m.group_id
   FROM (public.group_members m
     JOIN public.group_roles r ON ((r.group_id = m.group_id)));


--
-- Name: roles; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.roles (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    organisation_id uuid NOT NULL,
    system_key text,
    name public.citext NOT NULL,
    colour text,
    "position" integer DEFAULT 0 NOT NULL,
    permissions text[] DEFAULT '{}'::text[] NOT NULL,
    created_by uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT roles_colour_check CHECK ((colour = ANY (ARRAY['sky'::text, 'mint'::text, 'violet'::text, 'pink'::text, 'peach'::text, 'lime'::text, 'sun'::text, 'accent'::text]))),
    CONSTRAINT roles_name_check CHECK (((char_length((name)::text) >= 1) AND (char_length((name)::text) <= 100))),
    CONSTRAINT roles_permissions_check CHECK ((permissions <@ ARRAY['organisation.settings'::text, 'members.view'::text, 'members.invite'::text, 'members.manage'::text, 'groups.manage'::text, 'roles.assign'::text, 'journal.view'::text])),
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
-- Name: signing_keys; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.signing_keys (
    id uuid NOT NULL,
    purpose text NOT NULL,
    kid text NOT NULL,
    alg text NOT NULL,
    public_jwk jsonb NOT NULL,
    private_key bytea NOT NULL,
    private_nonce bytea NOT NULL,
    published_at timestamp with time zone DEFAULT now() NOT NULL,
    activated_at timestamp with time zone,
    retiring_at timestamp with time zone,
    removed_at timestamp with time zone,
    CONSTRAINT signing_keys_alg_check CHECK ((alg = ANY (ARRAY['ES256'::text, 'EdDSA'::text]))),
    CONSTRAINT signing_keys_phases_in_order CHECK ((((activated_at IS NULL) OR (activated_at >= published_at)) AND ((retiring_at IS NULL) OR ((activated_at IS NOT NULL) AND (retiring_at >= activated_at))) AND ((removed_at IS NULL) OR (retiring_at IS NOT NULL)))),
    CONSTRAINT signing_keys_purpose_check CHECK ((purpose = ANY (ARRAY['token'::text, 'events'::text])))
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
    avatar_key text,
    is_instance_admin boolean DEFAULT false NOT NULL,
    locale text,
    CONSTRAINT ck_users_locale CHECK (((locale IS NULL) OR (locale = ANY (ARRAY['fr'::text, 'en'::text, 'es'::text, 'de'::text, 'it'::text, 'pl'::text]))))
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
-- Name: changes seq; Type: DEFAULT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.changes ALTER COLUMN seq SET DEFAULT nextval('public.changes_seq_seq'::regclass);


--
-- Name: changes changes_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.changes
    ADD CONSTRAINT changes_id_key UNIQUE (id);


--
-- Name: changes changes_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.changes
    ADD CONSTRAINT changes_pkey PRIMARY KEY (seq);


--
-- Name: client_keys client_keys_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.client_keys
    ADD CONSTRAINT client_keys_pkey PRIMARY KEY (client_id, kid);


--
-- Name: group_members group_members_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.group_members
    ADD CONSTRAINT group_members_pkey PRIMARY KEY (group_id, user_id);


--
-- Name: group_roles group_roles_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.group_roles
    ADD CONSTRAINT group_roles_pkey PRIMARY KEY (group_id, role_id);


--
-- Name: groups groups_id_organisation_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.groups
    ADD CONSTRAINT groups_id_organisation_id_key UNIQUE (id, organisation_id);


--
-- Name: groups groups_organisation_id_name_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.groups
    ADD CONSTRAINT groups_organisation_id_name_key UNIQUE (organisation_id, name);


--
-- Name: groups groups_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.groups
    ADD CONSTRAINT groups_pkey PRIMARY KEY (id);


--
-- Name: invitations invitations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.invitations
    ADD CONSTRAINT invitations_pkey PRIMARY KEY (id);


--
-- Name: invitations invitations_token_hash_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.invitations
    ADD CONSTRAINT invitations_token_hash_key UNIQUE (token_hash);


--
-- Name: logout_deliveries logout_deliveries_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.logout_deliveries
    ADD CONSTRAINT logout_deliveries_pkey PRIMARY KEY (id);


--
-- Name: memberships memberships_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.memberships
    ADD CONSTRAINT memberships_pkey PRIMARY KEY (organisation_id, user_id);


--
-- Name: oauth_clients oauth_clients_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_clients
    ADD CONSTRAINT oauth_clients_pkey PRIMARY KEY (client_id);


--
-- Name: oauth_consents oauth_consents_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_consents
    ADD CONSTRAINT oauth_consents_pkey PRIMARY KEY (user_id, client_id);


--
-- Name: oauth_grants oauth_grants_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_grants
    ADD CONSTRAINT oauth_grants_pkey PRIMARY KEY (id);


--
-- Name: oauth_refresh_tokens oauth_refresh_tokens_grant_id_generation_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_refresh_tokens
    ADD CONSTRAINT oauth_refresh_tokens_grant_id_generation_key UNIQUE (grant_id, generation);


--
-- Name: oauth_refresh_tokens oauth_refresh_tokens_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_refresh_tokens
    ADD CONSTRAINT oauth_refresh_tokens_pkey PRIMARY KEY (digest);


--
-- Name: organisation_journal organisation_journal_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.organisation_journal
    ADD CONSTRAINT organisation_journal_pkey PRIMARY KEY (id);


--
-- Name: organisation_slugs organisation_slugs_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.organisation_slugs
    ADD CONSTRAINT organisation_slugs_pkey PRIMARY KEY (slug);


--
-- Name: organisations organisations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.organisations
    ADD CONSTRAINT organisations_pkey PRIMARY KEY (id);


--
-- Name: pairwise_subjects pairwise_subjects_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.pairwise_subjects
    ADD CONSTRAINT pairwise_subjects_pkey PRIMARY KEY (sector, user_id);


--
-- Name: pairwise_subjects pairwise_subjects_subject_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.pairwise_subjects
    ADD CONSTRAINT pairwise_subjects_subject_key UNIQUE (subject);


--
-- Name: recovery_codes recovery_codes_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recovery_codes
    ADD CONSTRAINT recovery_codes_pkey PRIMARY KEY (id);


--
-- Name: role_assignments role_assignments_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.role_assignments
    ADD CONSTRAINT role_assignments_pkey PRIMARY KEY (role_id, user_id);


--
-- Name: roles roles_id_organisation_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.roles
    ADD CONSTRAINT roles_id_organisation_id_key UNIQUE (id, organisation_id);


--
-- Name: roles roles_organisation_id_system_key_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.roles
    ADD CONSTRAINT roles_organisation_id_system_key_key UNIQUE (organisation_id, system_key);


--
-- Name: roles roles_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.roles
    ADD CONSTRAINT roles_pkey PRIMARY KEY (id);


--
-- Name: seaql_migrations seaql_migrations_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.seaql_migrations
    ADD CONSTRAINT seaql_migrations_pkey PRIMARY KEY (version);


--
-- Name: signing_keys signing_keys_kid_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.signing_keys
    ADD CONSTRAINT signing_keys_kid_key UNIQUE (kid);


--
-- Name: signing_keys signing_keys_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.signing_keys
    ADD CONSTRAINT signing_keys_pkey PRIMARY KEY (id);


--
-- Name: totp_secrets totp_secrets_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.totp_secrets
    ADD CONSTRAINT totp_secrets_pkey PRIMARY KEY (user_id);


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
-- Name: changes_unsent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX changes_unsent ON public.changes USING btree (seq) WHERE (sent_at IS NULL);


--
-- Name: client_keys_kid; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX client_keys_kid ON public.client_keys USING btree (kid);


--
-- Name: groups_external_id; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX groups_external_id ON public.groups USING btree (organisation_id, external_id) WHERE (external_id IS NOT NULL);


--
-- Name: idx_recovery_codes_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_recovery_codes_user ON public.recovery_codes USING btree (user_id);


--
-- Name: idx_users_email_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_users_email_unique ON public.users USING btree (email);


--
-- Name: idx_webauthn_credential_id_unique; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX idx_webauthn_credential_id_unique ON public.webauthn_credentials USING btree (credential_id);


--
-- Name: invitations_organisation; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX invitations_organisation ON public.invitations USING btree (organisation_id, created_at);


--
-- Name: logout_deliveries_due; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX logout_deliveries_due ON public.logout_deliveries USING btree (next_attempt_at) WHERE (delivered_at IS NULL);


--
-- Name: memberships_expires_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX memberships_expires_at ON public.memberships USING btree (expires_at) WHERE (expires_at IS NOT NULL);


--
-- Name: memberships_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX memberships_user ON public.memberships USING btree (user_id);


--
-- Name: oauth_clients_resource; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX oauth_clients_resource ON public.oauth_clients USING btree (resource) WHERE ((resource IS NOT NULL) AND (disabled_at IS NULL));


--
-- Name: oauth_grants_sid; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX oauth_grants_sid ON public.oauth_grants USING btree (sid) WHERE ((revoked_at IS NULL) AND (sid IS NOT NULL));


--
-- Name: oauth_grants_user; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX oauth_grants_user ON public.oauth_grants USING btree (user_id) WHERE (revoked_at IS NULL);


--
-- Name: organisation_journal_at; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX organisation_journal_at ON public.organisation_journal USING btree (organisation_id, at DESC, id DESC);


--
-- Name: organisation_slugs_organisation; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX organisation_slugs_organisation ON public.organisation_slugs USING btree (organisation_id);


--
-- Name: organisations_slug; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX organisations_slug ON public.organisations USING btree (slug);


--
-- Name: role_assignments_member; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX role_assignments_member ON public.role_assignments USING btree (organisation_id, user_id);


--
-- Name: roles_name; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX roles_name ON public.roles USING btree (organisation_id, name) WHERE (system_key IS NULL);


--
-- Name: signing_keys_one_active; Type: INDEX; Schema: public; Owner: -
--

CREATE UNIQUE INDEX signing_keys_one_active ON public.signing_keys USING btree (purpose) WHERE ((activated_at IS NOT NULL) AND (retiring_at IS NULL));


--
-- Name: organisation_journal organisation_journal_append_only; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER organisation_journal_append_only BEFORE DELETE OR UPDATE ON public.organisation_journal FOR EACH ROW EXECUTE FUNCTION public.organisation_journal_append_only();


--
-- Name: organisations organisations_system_roles; Type: TRIGGER; Schema: public; Owner: -
--

CREATE TRIGGER organisations_system_roles AFTER INSERT ON public.organisations FOR EACH ROW EXECUTE FUNCTION public.organisations_system_roles();


--
-- Name: client_keys client_keys_client_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.client_keys
    ADD CONSTRAINT client_keys_client_id_fkey FOREIGN KEY (client_id) REFERENCES public.oauth_clients(client_id) ON DELETE CASCADE;


--
-- Name: recovery_codes fk_recovery_codes_user; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.recovery_codes
    ADD CONSTRAINT fk_recovery_codes_user FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: totp_secrets fk_totp_secrets_user; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.totp_secrets
    ADD CONSTRAINT fk_totp_secrets_user FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: webauthn_credentials fk_webauthn_credentials_user; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.webauthn_credentials
    ADD CONSTRAINT fk_webauthn_credentials_user FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: group_members group_members_added_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.group_members
    ADD CONSTRAINT group_members_added_by_fkey FOREIGN KEY (added_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: group_members group_members_group_id_organisation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.group_members
    ADD CONSTRAINT group_members_group_id_organisation_id_fkey FOREIGN KEY (group_id, organisation_id) REFERENCES public.groups(id, organisation_id) ON DELETE CASCADE;


--
-- Name: group_members group_members_organisation_id_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.group_members
    ADD CONSTRAINT group_members_organisation_id_user_id_fkey FOREIGN KEY (organisation_id, user_id) REFERENCES public.memberships(organisation_id, user_id) ON DELETE CASCADE;


--
-- Name: group_roles group_roles_group_id_organisation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.group_roles
    ADD CONSTRAINT group_roles_group_id_organisation_id_fkey FOREIGN KEY (group_id, organisation_id) REFERENCES public.groups(id, organisation_id) ON DELETE CASCADE;


--
-- Name: group_roles group_roles_role_id_organisation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.group_roles
    ADD CONSTRAINT group_roles_role_id_organisation_id_fkey FOREIGN KEY (role_id, organisation_id) REFERENCES public.roles(id, organisation_id) ON DELETE CASCADE;


--
-- Name: groups groups_created_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.groups
    ADD CONSTRAINT groups_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: groups groups_organisation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.groups
    ADD CONSTRAINT groups_organisation_id_fkey FOREIGN KEY (organisation_id) REFERENCES public.organisations(id) ON DELETE CASCADE;


--
-- Name: invitations invitations_created_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.invitations
    ADD CONSTRAINT invitations_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: invitations invitations_organisation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.invitations
    ADD CONSTRAINT invitations_organisation_id_fkey FOREIGN KEY (organisation_id) REFERENCES public.organisations(id) ON DELETE CASCADE;


--
-- Name: invitations invitations_role_id_organisation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.invitations
    ADD CONSTRAINT invitations_role_id_organisation_id_fkey FOREIGN KEY (role_id, organisation_id) REFERENCES public.roles(id, organisation_id) ON DELETE CASCADE;


--
-- Name: logout_deliveries logout_deliveries_client_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.logout_deliveries
    ADD CONSTRAINT logout_deliveries_client_id_fkey FOREIGN KEY (client_id) REFERENCES public.oauth_clients(client_id) ON DELETE CASCADE;


--
-- Name: memberships memberships_invited_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.memberships
    ADD CONSTRAINT memberships_invited_by_fkey FOREIGN KEY (invited_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: memberships memberships_organisation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.memberships
    ADD CONSTRAINT memberships_organisation_id_fkey FOREIGN KEY (organisation_id) REFERENCES public.organisations(id) ON DELETE CASCADE;


--
-- Name: memberships memberships_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.memberships
    ADD CONSTRAINT memberships_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: oauth_consents oauth_consents_client_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_consents
    ADD CONSTRAINT oauth_consents_client_id_fkey FOREIGN KEY (client_id) REFERENCES public.oauth_clients(client_id) ON DELETE CASCADE;


--
-- Name: oauth_consents oauth_consents_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_consents
    ADD CONSTRAINT oauth_consents_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: oauth_grants oauth_grants_client_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_grants
    ADD CONSTRAINT oauth_grants_client_id_fkey FOREIGN KEY (client_id) REFERENCES public.oauth_clients(client_id) ON DELETE CASCADE;


--
-- Name: oauth_grants oauth_grants_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_grants
    ADD CONSTRAINT oauth_grants_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: oauth_refresh_tokens oauth_refresh_tokens_grant_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.oauth_refresh_tokens
    ADD CONSTRAINT oauth_refresh_tokens_grant_id_fkey FOREIGN KEY (grant_id) REFERENCES public.oauth_grants(id) ON DELETE CASCADE;


--
-- Name: organisation_journal organisation_journal_actor_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.organisation_journal
    ADD CONSTRAINT organisation_journal_actor_id_fkey FOREIGN KEY (actor_id) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: organisation_journal organisation_journal_organisation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.organisation_journal
    ADD CONSTRAINT organisation_journal_organisation_id_fkey FOREIGN KEY (organisation_id) REFERENCES public.organisations(id) ON DELETE CASCADE;


--
-- Name: organisation_slugs organisation_slugs_organisation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.organisation_slugs
    ADD CONSTRAINT organisation_slugs_organisation_id_fkey FOREIGN KEY (organisation_id) REFERENCES public.organisations(id) ON DELETE CASCADE;


--
-- Name: organisations organisations_created_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.organisations
    ADD CONSTRAINT organisations_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: pairwise_subjects pairwise_subjects_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.pairwise_subjects
    ADD CONSTRAINT pairwise_subjects_user_id_fkey FOREIGN KEY (user_id) REFERENCES public.users(id) ON DELETE CASCADE;


--
-- Name: role_assignments role_assignments_organisation_id_user_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.role_assignments
    ADD CONSTRAINT role_assignments_organisation_id_user_id_fkey FOREIGN KEY (organisation_id, user_id) REFERENCES public.memberships(organisation_id, user_id) ON DELETE CASCADE;


--
-- Name: role_assignments role_assignments_role_id_organisation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.role_assignments
    ADD CONSTRAINT role_assignments_role_id_organisation_id_fkey FOREIGN KEY (role_id, organisation_id) REFERENCES public.roles(id, organisation_id) ON DELETE CASCADE;


--
-- Name: roles roles_created_by_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.roles
    ADD CONSTRAINT roles_created_by_fkey FOREIGN KEY (created_by) REFERENCES public.users(id) ON DELETE SET NULL;


--
-- Name: roles roles_organisation_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.roles
    ADD CONSTRAINT roles_organisation_id_fkey FOREIGN KEY (organisation_id) REFERENCES public.organisations(id) ON DELETE CASCADE;


--
-- PostgreSQL database dump complete
--


