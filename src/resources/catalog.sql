WITH ext_types AS (
	SELECT objid
	FROM pg_depend
	JOIN pg_extension e ON refclassid = 'pg_extension'::regclass AND refobjid = e.oid
	WHERE classid = 'pg_type'::regclass
)
SELECT
    oid,
    typname as "name",
    typowner,
    CASE 
        WHEN typcategory = 'A' THEN 'a'
    	ELSE typtype 
	END AS "kind",
	-- Base type for domains, arrays and ranges
    CASE 
    	WHEN typtype = 'd' THEN typbasetype
    	WHEN typtype = 'r' THEN (SELECT r.rngsubtype FROM pg_range r WHERE oid = r.rngtypid)
        WHEN typcategory = 'A' THEN typelem
    	ELSE 0
	END AS "base_type"
FROM pg_type
WHERE
	(typrelid = 0 and typnamespace = (SELECT oid FROM pg_namespace WHERE nspname = 'pg_catalog'))
	OR oid IN (SELECT objid FROM ext_types)
ORDER BY oid
