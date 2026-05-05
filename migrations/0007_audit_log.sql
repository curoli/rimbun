create table audit_log (
  id uuid primary key,
  actor_user_id uuid references users(id),
  event_type text not null,
  target_type text not null,
  target_id uuid,
  payload jsonb not null default '{}'::jsonb,
  created_at timestamptz not null default now()
);
