- [x] switch student users to use git-shell as their login shell to prevent accidents
- [ ]Set up mTLS with Caddy
- [x] gradecope-runner
- [ ] gradecope-switchboard job dispatch
- [ ] gradecope-swtichboard admin socket
- [ ] gradecope-proxied-cli (Joseph)
  - `gradecope-proxied-cli history [<jobspec>]`.
    List prior jobs
  - `gradecope-proxied-cli status <jobspec>-<run_no>`.
    Get the status of a job
  - `gradecope-proxied-cli log <jobspec>-<run_no>`.
    Download and print out the log from a run
  - `gradecope-proxied-cli cancel <jobspec>-<run_no>`.
    Cancel a job
  - `gradecope-proxied-cli grades`.
    List out currently released labs and the user's checkoff status
- [x] arrange domain
- [ ] postgres backups
- [ ] reset filesystem using JTAG and a stub program