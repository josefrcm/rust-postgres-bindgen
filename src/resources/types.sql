select
	-- Common fields for all kind of types
	t.oid,
	n.nspname AS "schema",
    t.typname AS "name",--pg_catalog.format_type ( t.oid, NULL ) AS "name",
    coalesce ( pg_catalog.obj_description ( t.oid, 'pg_type' ), '' ) AS "description",
    CASE 
        WHEN t.typcategory = 'A' THEN 'a'
    	ELSE t.typtype 
	END AS "kind",
	-- For composites, is this a type, a table, or a view?
	exists (select * from pg_catalog.pg_tables pt where pt.schemaname = n.nspname and pt.tablename = t.typname) as is_table,
	exists (select * from pg_catalog.pg_views pv where pv.schemaname = n.nspname and pv.viewname = t.typname) or 
	exists (select * from pg_catalog.pg_matviews pm where pm.schemaname = n.nspname and pm.matviewname = t.typname) as is_view,
    -- Enum values,
    array(SELECT e.enumlabel FROM pg_enum e WHERE t.oid = e.enumtypid) AS "enum_values",
    -- Composite fields
    (SELECT coalesce(jsonb_agg(field), '[]'::jsonb) FROM (
    	SELECT 
            attrib.attname::text AS "name",
            --attrib.attnum AS "pos",
            --pg_catalog.format_type ( a.atttypid, NULL ) AS "typ",
            attrib.atttypid::int8 as "typ",
            --n2.nspname as "type_schema",
            --t2.typname as "type_name",            
            NOT attrib.attnotnull AS "is_nullable",
            --not attrib.attnotnull AS "is_nullable",
            pg_catalog.col_description ( attrib.attrelid, attrib.attnum ) AS "description"
        FROM pg_catalog.pg_attribute attrib
        --join pg_type t2 on attrib.atttypid = t2.oid
        --JOIN pg_catalog.pg_namespace n2 ON n2.oid = t2.typnamespace
        WHERE attrib.attrelid = t.typrelid AND attrib.attnum > 0 AND NOT attrib.attisdropped
        ORDER BY attrib.attnum 
    ) field) AS struct_fields,
    -- Base type for domains, arrays and ranges
    CASE 
    	WHEN t.typtype = 'd' THEN t.typbasetype
    	WHEN t.typtype = 'r' THEN (SELECT r.rngsubtype FROM pg_range r WHERE t.oid = r.rngtypid)
        WHEN t.typcategory = 'A' THEN t.typelem
    	ELSE 0
	END AS "base_type"
FROM pg_catalog.pg_type t
JOIN pg_catalog.pg_namespace n ON n.oid = t.typnamespace
LEFT JOIN pg_catalog.pg_class c ON c.oid = t.typrelid
WHERE
	(t.typrelid = 0 OR c.relkind IN ('e', 'c', 'd', 'r', 'a', 'v', 'm') )
    AND n.nspname <> 'information_schema'
    AND n.nspname NOT LIKE 'pg_%'
ORDER BY "schema", "name"
