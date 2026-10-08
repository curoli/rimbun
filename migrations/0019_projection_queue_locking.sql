-- Enqueueing must not acquire a section lock through a foreign-key check.
-- Section deletion explicitly removes its jobs below.
alter table projection_jobs drop constraint projection_jobs_section_id_fkey;

create or replace function queue_projection_job() returns trigger language plpgsql as $$
declare
    target_section uuid;
begin
    if TG_TABLE_NAME = 'submissions' then
        if TG_OP = 'UPDATE' and
            (NEW.section_id, NEW.user_id, NEW.markdown_content, NEW.status,
             NEW.published_at, NEW.superseded_by) is not distinct from
            (OLD.section_id, OLD.user_id, OLD.markdown_content, OLD.status,
             OLD.published_at, OLD.superseded_by) then
            return null;
        end if;
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

create function remove_section_projection_job() returns trigger language plpgsql as $$
begin
    delete from projection_jobs where section_id = OLD.id;
    return null;
end;
$$;

create trigger sections_remove_projection_job
after delete on sections
for each row execute function remove_section_projection_job();
