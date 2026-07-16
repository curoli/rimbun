alter table comments
add column deleted_at timestamptz,
add column deleted_by uuid references users(id);

drop index comments_primary_submission_user_idx;
create unique index comments_primary_submission_user_idx
  on comments(submission_id, user_id)
  where is_primary = true and parent_comment_id is null and deleted_at is null;
