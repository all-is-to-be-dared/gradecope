- Set up mTLS with Caddy
- gradecope-runner
- gradecope-proxied-cli
  - `gradecope-proxied-cli history [<jobspec>]`
    List prior jobs
  - `gradecope-proxied-cli status <jobspec>-<run_no>`
    Get the status of a job
  - `gradecope-proxied-cli log <jobspec>-<run_no>`
    Download and print out the log from a run
  - `gradecope-proxied-cli cancel <jobspec>-<run_no>`
    Cancel a job
  - `gradecope-proxied-cli grades`
    List out currently released labs and the user's checkoff status
- domain: gradecope.*****.net
- postgres backups
- gradecope-device-reset
  - reset filesystem using JTAG and a stub program
