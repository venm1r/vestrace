CREATE FUNCTION public.vestrace_safety_ed25519_verify(message BYTEA, signature BYTEA, public_key BYTEA)
RETURNS BOOLEAN
AS 'MODULE_PATHNAME', 'vestrace_safety_ed25519_verify'
LANGUAGE C IMMUTABLE STRICT PARALLEL SAFE;

REVOKE ALL ON FUNCTION public.vestrace_safety_ed25519_verify(BYTEA, BYTEA, BYTEA) FROM PUBLIC;
REVOKE ALL ON FUNCTION public.vestrace_safety_ed25519_verify(BYTEA, BYTEA, BYTEA) FROM vestrace;
REVOKE ALL ON FUNCTION public.vestrace_safety_ed25519_verify(BYTEA, BYTEA, BYTEA) FROM vestrace_safety_supervisor;