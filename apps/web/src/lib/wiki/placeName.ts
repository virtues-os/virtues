/**
 * A place's name, when it has one.
 *
 * A place nobody named is filed under its coordinates, "Location 41.8781,
 * -87.6298", and a visit with no place at all reads "Unknown". Neither is a
 * name, and printing either beside a person's day reads as if it were one.
 */

const COORDINATE_STUB = /^Location\s+-?\d+(\.\d+)?,\s*-?\d+(\.\d+)?$/;

/** The name, or null when it is the coordinate stub, "Unknown" or empty. */
export function realPlaceName(name: string | null | undefined): string | null {
	const n = name?.trim();
	if (!n || COORDINATE_STUB.test(n) || n.toLowerCase() === "unknown") return null;
	return n;
}

/** The name to print for a stop: its real name, else "Unnamed place". */
export function placeLabel(name: string | null | undefined): string {
	return realPlaceName(name) ?? "Unnamed place";
}
