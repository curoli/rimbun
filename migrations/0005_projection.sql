create table section_projection_runs (
  id uuid primary key,
  section_id uuid not null references sections(id) on delete cascade,
  status text not null check (status in ('pending', 'running', 'completed', 'failed')),
  started_at timestamptz,
  finished_at timestamptz,
  error text
);

create table section_projection_items (
  section_id uuid not null references sections(id) on delete cascade,
  submission_id uuid not null references submissions(id) on delete cascade,
  role text not null check (role in ('main', 'principal_alternative', 'other')),
  rank integer not null,
  cluster_id text,
  score double precision,
  primary key (section_id, submission_id)
);

create index section_projection_items_section_rank_idx on section_projection_items(section_id, rank);
