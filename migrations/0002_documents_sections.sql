create table documents (
  id uuid primary key,
  slug text not null unique,
  title text not null,
  visibility text not null check (visibility in ('public', 'authenticated')),
  markdown_policy jsonb not null default '{}'::jsonb,
  created_by uuid not null references users(id),
  created_at timestamptz not null default now()
);

create table sections (
  id uuid primary key,
  document_id uuid not null references documents(id) on delete cascade,
  parent_id uuid references sections(id) on delete cascade,
  title text not null,
  position integer not null,
  path text not null,
  created_at timestamptz not null default now()
);

create index sections_document_id_idx on sections(document_id);
create index sections_parent_id_idx on sections(parent_id);
create unique index sections_sibling_position_idx on sections(document_id, parent_id, position);
