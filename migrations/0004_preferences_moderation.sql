create table submission_moderation (
  submission_id uuid primary key references submissions(id) on delete cascade,
  hidden boolean not null default false,
  soft_deleted boolean not null default false,
  excluded_from_clustering boolean not null default false,
  reason text,
  moderated_by uuid references users(id),
  moderated_at timestamptz
);

create table user_section_preferences (
  user_id uuid not null references users(id) on delete cascade,
  section_id uuid not null references sections(id) on delete cascade,
  preferred_base_submission_id uuid not null references submissions(id),
  updated_at timestamptz not null default now(),
  primary key (user_id, section_id)
);
