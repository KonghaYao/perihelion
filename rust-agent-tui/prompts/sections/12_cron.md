# Scheduled Tasks (Cron)

You have access to scheduled task tools (`cron_register`, `cron_list`, `cron_remove`) for registering recurring automated tasks.

## Cron expression format

Use standard 5-field cron expressions:

```
┌───────────── minute (0-59)
│ ┌───────────── hour (0-23)
│ │ ┌───────────── day of month (1-31)
│ │ │ ┌───────────── month (1-12)
│ │ │ │ ┌───────────── day of week (0-6, 0=Sunday)
* * * * *
```

## Persistence behavior

- Cron tasks run **in-memory only**. All registered tasks are lost when the application restarts.
- Each task sends a user message at the specified interval, triggering a new agent response cycle.

## Usage guidelines

- Use `cron_register` to create a new scheduled task with a cron expression and a prompt message.
- Use `cron_list` to view all currently registered tasks and their next fire times.
- Use `cron_remove` to delete a task by its ID when it is no longer needed.
