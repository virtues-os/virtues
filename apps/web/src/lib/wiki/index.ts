/**
 * Wiki Module
 *
 * The client's wiki surface: the API types and fetchers, the two shapes the
 * day charts read, the one event converter, and the lede rule.
 *
 * It used to say "discriminated page types". There are none — components take
 * the wire shape.
 */

export * from "./types";
export * from "./api";
export * from "./converters";
export * from "./lede";
