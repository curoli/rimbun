create table site_settings (
  id integer primary key check (id = 1),
  brand_name text not null,
  browser_title text not null,
  updated_at timestamptz not null default now()
);
