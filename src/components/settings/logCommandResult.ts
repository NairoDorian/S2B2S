/** The shape a tauri-specta `typedError` command resolves to. */
type CommandResult<T, E> =
  | { status: "ok"; data: T }
  | { status: "error"; error: E };

/**
 * Awaits a fire-and-forget command (suspend / resume bindings, start / stop
 * key recording) and logs its failure. A `typedError` command resolves to
 * `{ status: "error" }` instead of rejecting, so a bare `.catch(console.error)`
 * only ever saw IPC failures and dropped the command's own error.
 */
export const logCommandResult = async <T, E>(
  label: string,
  call: Promise<CommandResult<T, E>>,
): Promise<void> => {
  try {
    const result = await call;
    if (result.status === "error") console.error(`${label}:`, result.error);
  } catch (error) {
    console.error(`${label}:`, error);
  }
};
