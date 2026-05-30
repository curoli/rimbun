create table variant_collections (
  id uuid primary key,
  name text not null,
  description text not null default '',
  created_by uuid not null references users(id),
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create table variant_entries (
  id uuid primary key,
  collection_id uuid not null references variant_collections(id) on delete cascade,
  position integer not null,
  label text not null,
  username_hint text,
  markdown_content text not null,
  created_at timestamptz not null default now(),
  updated_at timestamptz not null default now()
);

create index variant_entries_collection_id_idx on variant_entries(collection_id);
create unique index variant_entries_collection_position_idx on variant_entries(collection_id, position);

create table test_runs (
  id uuid primary key,
  collection_id uuid not null references variant_collections(id) on delete cascade,
  document_id uuid references documents(id) on delete set null,
  section_id uuid references sections(id) on delete set null,
  status text not null check (status in ('active', 'deleted')),
  created_by uuid not null references users(id),
  created_at timestamptz not null default now(),
  finished_at timestamptz,
  deleted_at timestamptz
);

create index test_runs_collection_id_idx on test_runs(collection_id);
create index test_runs_created_by_idx on test_runs(created_by);

create table test_run_users (
  test_run_id uuid not null references test_runs(id) on delete cascade,
  user_id uuid not null references users(id) on delete cascade,
  variant_entry_id uuid not null references variant_entries(id) on delete cascade,
  primary key (test_run_id, user_id),
  unique (test_run_id, variant_entry_id)
);

create index test_run_users_user_id_idx on test_run_users(user_id);
