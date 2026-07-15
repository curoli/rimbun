alter table site_settings
add column if not exists default_language text not null default 'en';

update site_settings
set default_language = 'en'
where default_language not in ('de', 'en');

alter table site_settings
add constraint site_settings_default_language_check
check (default_language in ('de', 'en'));
