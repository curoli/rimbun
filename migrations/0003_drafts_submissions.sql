create table submissions (
  id uuid primary key,
  section_id uuid not null references sections(id) on delete cascade,
  user_id uuid not null references users(id) on delete cascade,
  base_submission_id uuid references submissions(id),
  markdown_content text not null,
  status text not null check (status in ('published')),
  published_at timestamptz not null default now(),
  superseded_by uuid references submissions(id)
);

create index submissions_section_id_idx on submissions(section_id);
create index submissions_section_user_idx on submissions(section_id, user_id, published_at desc);

create table drafts (
  id uuid primary key,
  section_id uuid not null references sections(id) on delete cascade,
  user_id uuid not null references users(id) on delete cascade,
  base_submission_id uuid references submissions(id),
  markdown_content text not null,
  updated_at timestamptz not null default now(),
  unique (section_id, user_id)
);
