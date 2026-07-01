alter table site_settings
add column if not exists color_scheme text not null default 'amber-dawn';

update site_settings
set color_scheme = 'amber-dawn'
where color_scheme is null or btrim(color_scheme) = '';
