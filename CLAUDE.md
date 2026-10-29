# Project notes for Claude

## Clean up DDEV sites after testing

Delete every DDEV site you create for testing (smoke runs, click-tests, any
throwaway project) as soon as you are done with it. Always skip the snapshot:

```bash
ddev delete --omit-snapshot --yes <project-name>
```

Then remove the project folder. Without `--omit-snapshot`, DDEV saves a
database snapshot of every deleted site, and those pile up on disk.

Only delete sites you created in this session. Leave the user's own projects
alone, even if they look like test sites.
