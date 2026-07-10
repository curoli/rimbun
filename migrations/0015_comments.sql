alter table drafts
add column main_comment_markdown text;

create table comments (
  id uuid primary key,
  submission_id uuid not null references submissions(id) on delete cascade,
  parent_comment_id uuid references comments(id) on delete cascade,
  user_id uuid not null references users(id) on delete cascade,
  markdown_content text not null,
  is_primary boolean not null default false,
  created_at timestamptz not null default now(),
  check (parent_comment_id is null or is_primary = false)
);

create index comments_submission_idx on comments(submission_id, created_at asc);
create index comments_parent_idx on comments(parent_comment_id, created_at asc);
create unique index comments_primary_submission_user_idx
  on comments(submission_id, user_id)
  where is_primary = true and parent_comment_id is null;
