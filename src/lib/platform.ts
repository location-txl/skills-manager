/**
 * The operating system, as the webview sees it.
 *
 * There is no OS plugin in this project, so the user agent is the signal — the
 * platform-specific UI elsewhere already reads it the same way.
 */
export const IS_MACOS = navigator.userAgent.includes("Mac");
export const IS_WINDOWS = navigator.userAgent.includes("Windows");
