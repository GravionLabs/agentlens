// The agent CLIs the loop can start. The project config names one of them (`deliver.harness`); the
// command line is built here, never read from a repository, so a change in a repository cannot make
// the loop run another program.

export type HarnessInput = {
  /** The slash command of the skill with its arguments. */
  prompt: string;
  /** Undefined: the default of the harness. */
  model?: string;
  /** The reasoning effort, one of `efforts`. Undefined: the default of the harness. */
  effort?: string;
};

export type Harness = {
  /** The model the loop reports when none is given. */
  defaultModel: string;
  command(input: HarnessInput): string[];
  /** The review session takes the first, the second when the implementer runs a model of the first's family. */
  reviewModels: [string, string];
  efforts: string[];
  /** The stream is the stream-json of Claude Code: the loop reads steps, cost and context from it. */
  claudeStream: boolean;
};

export const HARNESSES: Record<string, Harness> = {
  claude: {
    defaultModel: 'sonnet',
    claudeStream: true,
    reviewModels: ['opus', 'sonnet'],
    efforts: ['low', 'medium', 'high', 'xhigh', 'max'],
    command: ({ prompt, model, effort }) => [
      'claude',
      '-p',
      prompt,
      '--model',
      model ?? 'sonnet',
      ...(effort ? ['--effort', effort] : []),
      '--permission-mode',
      'auto',
      '--output-format',
      'stream-json',
      '--verbose',
      // a wakeup keeps the process alive after the result
      '--disallowedTools',
      'ScheduleWakeup',
      'CronCreate',
    ],
  },
  // Flags checked against `copilot --help` (Copilot CLI 1.0.116). No stream format: the loop logs plain lines.
  copilot: {
    defaultModel: 'auto',
    claudeStream: false,
    reviewModels: ['claude-opus-5.5', 'claude-sonnet-5.5'],
    efforts: ['none', 'minimal', 'low', 'medium', 'high', 'xhigh', 'max'],
    command: ({ prompt, model, effort }) => [
      'copilot',
      '-p',
      prompt,
      ...(model ? ['--model', model] : []),
      ...(effort ? ['--reasoning-effort', effort] : []),
      '--allow-all-tools',
      '--allow-all-paths',
      '--allow-all-urls',
      '--no-ask-user',
      '--no-color',
    ],
  },
};
