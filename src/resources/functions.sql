-- select
--     ns.nspname as "schema",
--     p.proname as "name",
--     p.prokind as "kind",
--     p.proisstrict as "is_strict",
--     p.proargnames as "arg_names",
--     array(select pg_catalog.format_type(unnest(p.proallargtypes), null)) as "allarg_types",
--     array(select pg_catalog.format_type(unnest(p.proargtypes), null)) as "arg_types",
--     p.proargmodes as "arg_modes",
--     pg_catalog.format_type(p.prorettype, null) as "ret_type",
--     p.proretset as "ret_set"
-- from pg_proc p
-- join pg_namespace ns on (p.pronamespace = ns.oid)
-- where probin is null and pronamespace in (2200);

select 
    p.oid as "oid",
    ns.nspname as "schema",
    p.proname as "name",
    p.prokind as "kind",
    p.proisstrict as "is_strict",
    p.proargnames as "arg_names",
    p.proargmodes as "arg_modes",
    case when p.proargmodes is null
    	then array(select unnest(p.proargtypes))
    	else array(select unnest(p.proallargtypes))
	end as "arg_types",
    p.prorettype as "ret_type",
    p.proretset as "ret_set"
from pg_proc p
join pg_namespace ns on (p.pronamespace = ns.oid)
where probin is null and pronamespace in (2200);
