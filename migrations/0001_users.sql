create table users (
  id uuid primary key,
  username text not null unique,
  display_name text not null,
  email text not null unique,
  password_hash text not null,
  role text not null check (role in ('normal', 'privileged', 'admin')),
  created_at timestamptz not null default now()
);
