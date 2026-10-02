/**
 * One JSON line per event on stderr.
 *
 * The fields are fixed by each call site and none of them can carry a secret:
 * there is no field for a header, a body or an error's own text. The service
 * credential, a session token, a delegation and a device credential never reach
 * a log line, because nothing here is ever handed one.
 */
export type LogFields = Readonly<Record<string, string | number | boolean | undefined>>;

export type Log = (event: string, fields?: LogFields) => void;

export const stderrLog: Log = (event, fields = {}) => {
  process.stderr.write(`${JSON.stringify({ at: new Date().toISOString(), event, ...fields })}\n`);
};

export const silentLog: Log = () => {};
