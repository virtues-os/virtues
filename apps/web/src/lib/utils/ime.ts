/**
 * Whether a key belongs to an input method mid-composition (Japanese,
 * Chinese, Korean): the Enter that commits a candidate, the arrows that
 * choose one, the Escape that drops one. Chrome and Firefox mark such keys
 * `isComposing`; WebKit (Safari, and the Mac and iOS apps) sends the Enter
 * that commits as keyCode 229 with `isComposing` already false. A menu or a
 * field that acts on Enter, Tab, the arrows or Escape leaves these be.
 */
export function inComposition(e: KeyboardEvent): boolean {
	return e.isComposing || e.keyCode === 229;
}
