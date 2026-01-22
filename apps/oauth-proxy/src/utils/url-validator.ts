/**
 * URL validation utilities for OAuth proxy
 */

/**
 * Validate return URL to prevent open redirect attacks
 *
 * This function ensures that OAuth callbacks only redirect to approved domains
 * to protect against open redirect vulnerabilities.
 *
 * @param url - The URL to validate
 * @returns true if the URL is valid and from an allowed domain, false otherwise
 */
export function isValidReturnUrl(url: string): boolean {
  try {
    const parsed = new URL(url);

    // Allow localhost for development
    if (parsed.hostname === 'localhost' || parsed.hostname === '127.0.0.1') {
      return true;
    }

    // Allow specific domains (add your domain patterns here)
    const allowedPatterns = [
      /^.*\.virtues\.com$/,
      /^.*\.ngrok-free\.app$/, // Free ngrok domains
      /^.*\.ngrok\.io$/,       // Paid ngrok domains
      /^.*\.local$/,
      /^.*\.localhost$/,
    ];

    return allowedPatterns.some((pattern) => pattern.test(parsed.hostname));
  } catch {
    return false;
  }
}