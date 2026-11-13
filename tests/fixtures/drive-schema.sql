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


SET default_tablespace = '';

SET default_table_access_method = heap;

--
-- Name: file_links; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.file_links (
    id uuid NOT NULL,
    file_id uuid NOT NULL,
    organisation_id uuid NOT NULL,
    token text NOT NULL,
    created_by uuid,
    created_at timestamp with time zone NOT NULL,
    expires_at timestamp with time zone,
    password_hash text,
    download_count integer DEFAULT 0 NOT NULL,
    revoked_at timestamp with time zone
);


--
-- Name: file_rules; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.file_rules (
    id uuid NOT NULL,
    organisation_id uuid NOT NULL,
    file_id uuid NOT NULL,
    subject_kind text NOT NULL,
    subject_id uuid NOT NULL,
    allow text[] DEFAULT '{}'::text[] NOT NULL,
    deny text[] DEFAULT '{}'::text[] NOT NULL,
    created_by uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    CONSTRAINT file_rules_subject_kind_check CHECK ((subject_kind = ANY (ARRAY['role'::text, 'group'::text, 'person'::text])))
);


--
-- Name: file_stars; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.file_stars (
    account_id uuid NOT NULL,
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
    thumbnail_key text,
    mime_type text DEFAULT 'application/octet-stream'::text NOT NULL,
    image_width integer,
    image_height integer,
    created_by uuid,
    created_at timestamp with time zone DEFAULT now() NOT NULL
);


--
-- Name: files; Type: TABLE; Schema: public; Owner: -
--

CREATE TABLE public.files (
    id uuid NOT NULL,
    organisation_id uuid NOT NULL,
    owner_id uuid,
    name character varying NOT NULL,
    kind text DEFAULT 'file'::text NOT NULL,
    parent_folder_id uuid,
    current_version_id uuid,
    size_bytes bigint DEFAULT 0 NOT NULL,
    imported_source text,
    external_ref text,
    created_at timestamp with time zone DEFAULT now() NOT NULL,
    updated_at timestamp with time zone DEFAULT now() NOT NULL,
    deleted_at timestamp with time zone,
    deleted_by uuid,
    trashed boolean DEFAULT false NOT NULL,
    purged_at timestamp with time zone,
    holder text,
    holder_ref text,
    CONSTRAINT files_holder_ref_check CHECK ((char_length(holder_ref) <= 200)),
    CONSTRAINT files_kind_check CHECK ((kind = ANY (ARRAY['file'::text, 'file-text'::text, 'file-spreadsheet'::text, 'image'::text, 'folder'::text])))
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
-- Name: file_rules file_rules_file_id_subject_kind_subject_id_key; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_rules
    ADD CONSTRAINT file_rules_file_id_subject_kind_subject_id_key UNIQUE (file_id, subject_kind, subject_id);


--
-- Name: file_rules file_rules_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_rules
    ADD CONSTRAINT file_rules_pkey PRIMARY KEY (id);


--
-- Name: file_stars file_stars_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_stars
    ADD CONSTRAINT file_stars_pkey PRIMARY KEY (account_id, file_id);


--
-- Name: file_versions file_versions_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_versions
    ADD CONSTRAINT file_versions_pkey PRIMARY KEY (id);


--
-- Name: files files_pkey; Type: CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.files
    ADD CONSTRAINT files_pkey PRIMARY KEY (id);


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
-- Name: file_rules_file; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX file_rules_file ON public.file_rules USING btree (file_id);


--
-- Name: files_name_trgm; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX files_name_trgm ON public.files USING gin (name public.gin_trgm_ops);


--
-- Name: group_members_account; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX group_members_account ON public.group_members USING btree (organisation_id, account_id);


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
-- Name: idx_files_holder; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_files_holder ON public.files USING btree (holder, holder_ref);


--
-- Name: idx_files_organisation_parent; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_files_organisation_parent ON public.files USING btree (organisation_id, parent_folder_id);


--
-- Name: idx_files_trash; Type: INDEX; Schema: public; Owner: -
--

CREATE INDEX idx_files_trash ON public.files USING btree (organisation_id, deleted_at) WHERE trashed;


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
-- Name: file_versions file_versions_file_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.file_versions
    ADD CONSTRAINT file_versions_file_id_fkey FOREIGN KEY (file_id) REFERENCES public.files(id) ON DELETE CASCADE;


--
-- Name: files files_parent_folder_id_fkey; Type: FK CONSTRAINT; Schema: public; Owner: -
--

ALTER TABLE ONLY public.files
    ADD CONSTRAINT files_parent_folder_id_fkey FOREIGN KEY (parent_folder_id) REFERENCES public.files(id) ON DELETE SET NULL;


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


