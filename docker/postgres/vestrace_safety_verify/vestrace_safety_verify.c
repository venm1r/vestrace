#include "postgres.h"
#include "fmgr.h"
#include "varatt.h"
#include <sodium.h>

PG_MODULE_MAGIC;
PG_FUNCTION_INFO_V1(vestrace_safety_ed25519_verify);

Datum
vestrace_safety_ed25519_verify(PG_FUNCTION_ARGS)
{
    bytea *message = PG_GETARG_BYTEA_PP(0);
    bytea *signature = PG_GETARG_BYTEA_PP(1);
    bytea *public_key = PG_GETARG_BYTEA_PP(2);

    if (VARSIZE_ANY_EXHDR(signature) != crypto_sign_BYTES ||
        VARSIZE_ANY_EXHDR(public_key) != crypto_sign_PUBLICKEYBYTES ||
        sodium_init() < 0)
        PG_RETURN_BOOL(false);

    PG_RETURN_BOOL(crypto_sign_verify_detached(
        (const unsigned char *) VARDATA_ANY(signature),
        (const unsigned char *) VARDATA_ANY(message),
        (unsigned long long) VARSIZE_ANY_EXHDR(message),
        (const unsigned char *) VARDATA_ANY(public_key)) == 0);
}
