create table submission_embeddings (
  submission_id uuid primary key references submissions(id) on delete cascade,
  model_name text not null,
  embedding jsonb not null,
  created_at timestamptz not null default now()
);
