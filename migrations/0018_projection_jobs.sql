create table projection_jobs (
    section_id uuid primary key references sections(id) on delete cascade,
    revision bigint not null default 1,
    attempts integer not null default 0,
    available_at timestamptz not null default now(),
    lease_until timestamptz,
    lease_token uuid,
    last_error text,
    updated_at timestamptz not null default now()
);

create function queue_projection_job() returns trigger language plpgsql as $$
declare
    target_section uuid;
begin
    if TG_TABLE_NAME = 'submissions' then
        target_section := coalesce(NEW.section_id, OLD.section_id);
    else
        select section_id into target_section from submissions
        where id = coalesce(NEW.submission_id, OLD.submission_id);
    end if;
    if exists (select 1 from sections where id = target_section) then
        insert into projection_jobs(section_id) values (target_section)
        on conflict(section_id) do update set
            revision = projection_jobs.revision + 1,
            attempts = 0,
            available_at = now(),
            last_error = null,
            updated_at = now();
    end if;
    return null;
end;
$$;

create trigger submissions_queue_projection
after insert or update or delete on submissions
for each row execute function queue_projection_job();

create trigger moderation_queue_projection
after insert or update or delete on submission_moderation
for each row execute function queue_projection_job();

-- Repair existing projections as well, including ones left stale before migration.
insert into projection_jobs(section_id)
select distinct section_id from submissions;
