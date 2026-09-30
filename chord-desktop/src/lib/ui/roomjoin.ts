// Pure helpers for joining a room: the question before a room is made, the CAPTCHA of a
// join, and the choice to share a room password with the other devices.
import type { DataForm } from '$lib/chord/types';
import { localPart } from './adapt';
import { problems } from './forms';

/**
 * True when the error of a disco#info read says that the room does not exist
 * (`item-not-found`). A join then makes a new room, so the app asks first (XEP-0045, 10.1).
 * Any other error (a timeout, a refusal) does not stop the join.
 */
export function roomIsMissing(error: unknown): boolean {
  return (error as { code?: string } | null)?.code === 'itemNotFound';
}

/** The text of the question before a new room is made. */
export function createRoomQuestion(room: string): string {
  return (
    `There is no room at ${room}. If you join, the server makes a new room named ` +
    `${localPart(room)} and you own it. Check the address for a typo first.`
  );
}

/** What is wrong in a CAPTCHA answer. The form has the same rules as any data form. */
export function captchaProblems(form: DataForm): string[] {
  return problems(form);
}

/** The text of the question about the room password in the bookmark. */
export function sharePasswordQuestion(room: string): string {
  return (
    `Save the password of ${localPart(room)} in your bookmarks, so that your other devices ` +
    `join it too? The bookmarks stay private to your account, but the server keeps the ` +
    `password as plain text. If you say no, the password stays on this device only.`
  );
}
