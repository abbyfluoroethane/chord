// XEP-0245 "/me" actions. The body goes out as typed. Each client shows a body that
// starts with "/me " as an action line: "* Alice waves".

/** The text after "/me ", or null if the body is no action. */
export function actionText(body: string): string | null {
  if (!body.startsWith('/me ')) return null;
  const rest = body.slice(4).trim();
  return rest ? rest : null;
}

/** One line for a preview or a notification: "* Alice waves" for an action, else the body. */
export function actionLine(name: string, body: string): string {
  const rest = actionText(body);
  return rest === null ? body : `* ${name} ${rest}`;
}
