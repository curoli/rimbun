-- Preserve the start of an outstanding job across input revisions and retries.
alter table projection_jobs add column queued_at timestamptz;

-- The original enqueue time is unavailable for pre-existing jobs. Keep the
-- last known input time rather than making the entire backlog look new.
update projection_jobs set queued_at = updated_at;

alter table projection_jobs
    alter column queued_at set default now(),
    alter column queued_at set not null;

-- The enqueue trigger deliberately leaves queued_at unchanged on conflict.
-- Only removal of a completed job lets the next enqueue start a fresh interval.
