create table user_sessions (
  id uuid primary key,
  token text not null unique,
  user_id uuid not null references users(id) on delete cascade,
  created_at timestamptz not null default now(),
  expires_at timestamptz not null
);

create index user_sessions_user_id_idx on user_sessions(user_id);
create index user_sessions_expires_at_idx on user_sessions(expires_at);
