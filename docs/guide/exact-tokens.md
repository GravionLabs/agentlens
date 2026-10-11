# Exact token counts

By default the size of skill texts is estimated as characters divided by four, and nothing leaves your machine. With `--exact` the name and description of every installed skill (the baseline listing) and the body of each skill are counted by the Anthropic `count_tokens` API instead:

```sh
export ANTHROPIC_API_KEY=...
agentlens --exact skills
agentlens --exact --exact-model claude-opus-5-5 unused
```

This is the only feature of AgentLens that sends text to a server, and it is off unless you ask for it. The application never uses it; it always estimates.

- **Where it works:** `skills`, `context`, `unused`, `compare` and `report`. Without `ANTHROPIC_API_KEY` the command stops before reading anything. `--exact-model` picks the tokenizer (default `claude-sonnet-5-5`); Claude models do not all tokenize alike.
- **What is sent:** the text of each installed skill (name plus description, and the body), one request per distinct text, to `https://api.anthropic.com/v1/messages/count_tokens` (or to `ANTHROPIC_BASE_URL` if set). Session contents are never sent. Skill files can contain private instructions; do not use `--exact` if they must not leave the machine. The counting endpoint is free of charge but rate limited.
- **Cache:** counts are stored by model and content hash in `$XDG_CACHE_HOME/agentlens/count_tokens/<model>.json` (`~/.cache/...`), so repeated runs send nothing for unchanged skills. Delete the file to start over.
- **Marking:** counted values are shown without `~`. Values that are still computed stay estimates: carry, and the sizes of instruction files and tool definitions, which are known only by their file size.
- **Failure:** if the key is rejected, the network is down or the rate limit is hit, the run continues with the estimate, prints one warning on stderr and sends no further requests.
- The count includes the few tokens the API adds around a message, so it is slightly higher than the bare text.
