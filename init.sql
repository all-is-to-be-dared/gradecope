\c gradecope;

DROP SCHEMA public CASCADE;
CREATE SCHEMA public;
GRANT ALL ON SCHEMA public TO postgres;
GRANT ALL ON SCHEMA public TO public;

/* Users table.
 */
CREATE TABLE users (
    id UUID PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,

    UNIQUE (name)
);

/* Types of jobs.
 */
CREATE TABLE job_types (
    id UUID NOT NULL PRIMARY KEY,
    spec TEXT NOT NULL,

    UNIQUE (spec)
);

/* Job states.
 */
CREATE TYPE job_state AS ENUM(
    /* Job was submitted and is in queue */
    'submitted',
    /* Job was started on the test runner */
    'started',
    /* Job was canceled for some reason; this could be because of:
        - runner failure
        - user canceled job
        - job timed out in queue
     */
    'canceled',
    /* Job ran to completion */
    'completed' );

/* All submitted jobs, regardless of job status
 */
CREATE TABLE jobs (
    id
        UUID
        NOT NULL
        PRIMARY KEY,
    owner
        UUID
        NOT NULL
        REFERENCES users(id)
        ON DELETE RESTRICT,
    job_type
        UUID
        NOT NULL
        REFERENCES job_types(id),
    commit
        TEXT
        NOT NULL,

    /* state of the job */
    state job_state NOT NULL,
    
    /* time at which job was submitted */
    submit_timestamp
        TIMESTAMP WITHOUT TIME ZONE
        NOT NULL,
    /* time at which job started */
    start_timestamp
        TIMESTAMP WITHOUT TIME ZONE
        NULL
        DEFAULT NULL,
    /* time at which job was canceled */
    canceled_timestamp
        TIMESTAMP WITHOUT TIME ZONE
        NULL
        DEFAULT NULL,

    /* full text of the run log, set if state is started, finished, or possibly if canceled */
    run_log
        BYTEA
        NULL
        DEFAULT NULL,

    /* the reason the job was canceled */
    cancel_reason
        TEXT
        NULL
        DEFAULT NULL,

    /* result of the test */
    test_result
        TEXT
        NULL
        DEFAULT NULL,

    /* ------------ CHECK CONSTRAINTS ------------ */

    /* if state is started or finished, then start_timestamp is not null */
    CHECK(
        NOT( state = 'started' OR state = 'completed' )
        OR start_timestamp IS NOT NULL ),
    /* if state is submitted, then start_timestamp is null */
    CHECK(NOT( state = 'submitted') OR start_timestamp IS NULL ),
    /* if state is canceled, then run_log is null IFF start_timestamp is null */
    CHECK(
        NOT( state = 'canceled' )
        OR ( start_timestamp IS NULL ) = ( run_log is NULL ) ),
    /* if state is submitted or started, then run_log is null */
    CHECK(
        NOT( state = 'submitted' OR state = 'started' )
        OR run_log IS NULL ),
    /* state is canceled IFF canceled_timestamp is not null */
    CHECK( ( state = 'canceled' ) = ( canceled_timestamp IS NOT NULL ) ),
    CHECK( ( state = 'canceled' ) = ( cancel_reason IS NOT NULL ) ),
    CHECK( ( state = 'completed' ) = ( test_result IS NOT NULL ) )
);
