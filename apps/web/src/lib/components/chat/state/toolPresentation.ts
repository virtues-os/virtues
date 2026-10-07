/**
 * How each tool reads in the thinking block: one entry per tool, in one place.
 *
 * Three tables used to answer this — depth, the noun for the collapsed
 * header, and a switch of descriptions — and a comment claimed they could not
 * drift. They had: `skip_step` rendered as "Skip step", and eleven tools
 * reached the header under their raw ids. `tool-ids.json` is written from the
 * box's registry (`virtues-registry` tools.rs), and the test beside this file
 * fails when a tool there has no entry here.
 *
 * DEPTH is how far a call goes, in ThinkingMark's dots: 3 reasons in place
 * with what is loaded, 4 goes out to something (the record, the web, a file,
 * an applet), 5 is the fan-out itself. The WORDS say what kind of work it is.
 * Keeping those orthogonal is what stops this needing twenty states.
 */

export interface ToolPart {
	type: string;
	toolCallId?: string;
	toolName?: string;
	input?: Record<string, unknown>;
	state?: string;
	output?: unknown;
	errorText?: string;
}

/**
 * Where one call stands. `unfinished` is a call the turn ended without
 * answering — stopped, cut off, or skipped on the last step — which used to
 * spin forever on a turn that was long over.
 */
export type ToolStatus = "running" | "done" | "failed" | "unfinished";

/** What the box records for a call its turn ended without answering
 *  (`turn_recorder.rs`); the model's own replay says the same words. */
export const TOOL_UNFINISHED = "the tool did not finish";
/** What it records for a call running when Stop was pressed (`agent/turn.rs`
 *  `stopped`), inside its failure envelope. A stop is not a failure. */
const TOOL_STOPPED = "stopped by the owner before it finished";

export function toolStatus(part: ToolPart, turnWorking: boolean): ToolStatus {
	if (part.state === "output-available") return "done";
	if (part.state === "output-error") {
		const text = part.errorText ?? "";
		return text === TOOL_UNFINISHED || text.includes(TOOL_STOPPED) ? "unfinished" : "failed";
	}
	// input-streaming, input-available, an approval, or no state at all: the
	// call has not answered. While the turn runs that is a call in progress;
	// once it has ended, it is one that never will.
	return turnWorking ? "running" : "unfinished";
}

export function toolName(part: ToolPart): string {
	if (part.toolName) return part.toolName;
	if (part.type?.startsWith("tool-")) return part.type.slice(5);
	return part.type || "tool";
}

type Describe = (input: Record<string, unknown>, live: boolean, short: boolean) => string;

interface Presentation {
	/** For the collapsed header: "your records, the web". */
	noun: string;
	depth: 3 | 4 | 5;
	/** `[live, done]`, or a function when the arguments change the line. */
	say: [string, string] | Describe;
}

const TOOLS: Record<string, Presentation> = {
	// Rendered as its own thought in the list, never as a described call.
	think: { noun: "thinking", depth: 3, say: ["Planning", "Planned"] },
	code_interpreter: {
		noun: "a calculation",
		depth: 3,
		say: ["Working something out", "Worked something out"],
	},
	web_search: {
		noun: "the web",
		depth: 4,
		// The query streams in after the call starts; until it has, say less.
		say: (input, live, short) => {
			const verb = live ? "Searching" : "Searched";
			const query = typeof input.query === "string" ? input.query.trim() : "";
			return query ? `${verb} the web for "${fit(query, short)}"` : `${verb} the web`;
		},
	},
	semantic_search: {
		noun: "your records",
		depth: 4,
		// The tool takes `queries` (up to four phrasings of one need) and keeps
		// `query` for back-compat; its own description tells the model to prefer
		// the array. Same precedence the tool applies. The other phrasings
		// become a count: four wordings of one question are noise to read.
		say: (input, live, short) => {
			const list = (Array.isArray(input.queries) ? (input.queries as unknown[]) : []).filter(
				(q): q is string => typeof q === "string" && q.trim() !== "",
			);
			if (list.length === 0 && typeof input.query === "string" && input.query.trim()) {
				list.push(input.query);
			}
			const verb = live ? "Searching" : "Searched";
			if (list.length === 0) return `${verb} your records`;
			const more = list.length > 1 ? ` +${list.length - 1} more` : "";
			return `${verb} your records for "${fit(list[0], short)}"${more}`;
		},
	},
	sql_query: {
		noun: "your data",
		depth: 4,
		say: (input, live) => {
			const op = input.operation as string;
			if (op === "list_tables") {
				return live ? "Listing what data there is" : "Listed what data there is";
			}
			if (op === "get_schema") {
				const verb = live ? "Checking the shape of" : "Checked the shape of";
				const tables = input.tables as string[] | undefined;
				if (!tables?.length) return `${verb} a table`;
				const shown = tables.slice(0, 2).map(plainTableName).join(", ");
				const more = tables.length > 2 ? ` +${tables.length - 2} more` : "";
				return `${verb} ${shown}${more}`;
			}
			const sql = (input.sql as string) || "";
			const table = plainTableName(sql.match(/FROM\s+([a-z_]+)/i)?.[1] ?? "");
			const verb = live ? "Reading" : "Read";
			return `${verb} your ${table || "data"}`;
		},
	},
	sql_write: {
		noun: "your records",
		depth: 4,
		say: ["Writing to your records", "Wrote to your records"],
	},
	shell: {
		noun: "your server",
		depth: 4,
		// The command itself, not a paraphrase: in sudo mode this line is the
		// owner's record of what ran on their server.
		say: (input, live, short) => {
			const command = ((input.command as string) || "").trim().split("\n")[0];
			if (!command) return live ? "Running a command" : "Ran a command";
			return `${live ? "Running" : "Ran"} ${short ? clip(command) : clip(command, 90)}`;
		},
	},
	read_asset: { noun: "a file", depth: 4, say: ["Opening a file", "Opened a file"] },
	// The owner's browser, a window in the Mac app.
	browser_open: {
		noun: "your browser",
		depth: 4,
		say: (input, live) => {
			let host = "";
			try {
				host = new URL(String(input.url ?? "")).hostname.replace(/^www\./, "");
			} catch {
				host = "";
			}
			const verb = live ? "Opening" : "Opened";
			return host ? `${verb} ${host} in your browser` : `${verb} a page in your browser`;
		},
	},
	browser_snapshot: { noun: "your browser", depth: 4, say: ["Reading the page", "Read the page"] },
	browser_click: { noun: "your browser", depth: 4, say: ["Clicking", "Clicked"] },
	browser_type: { noun: "your browser", depth: 4, say: ["Typing", "Typed"] },
	browser_press: {
		noun: "your browser",
		depth: 4,
		say: (input, live) => `${live ? "Pressing" : "Pressed"} ${String(input.key ?? "a key")}`,
	},
	browser_scroll: { noun: "your browser", depth: 4, say: ["Scrolling", "Scrolled"] },
	browser_screenshot: { noun: "your browser", depth: 4, say: ["Looking at the page", "Looked at the page"] },
	browser_handoff: {
		noun: "your browser",
		depth: 4,
		say: (input, live) => {
			const reason = String(input.reason ?? "").trim();
			if (live) return reason ? `Waiting for you: ${reason}` : "Waiting for you in the browser";
			return "You took a step in the browser";
		},
	},
	get_page_content: { noun: "a page", depth: 4, say: ["Reading a page", "Read a page"] },
	create_page: { noun: "a new page", depth: 4, say: ["Writing a new page", "Wrote a new page"] },
	edit_page: { noun: "a page", depth: 4, say: ["Editing a page", "Edited a page"] },
	revise_article: {
		noun: "an article",
		depth: 4,
		say: ["Revising an article", "Revised an article"],
	},
	write_it_up: { noun: "an article", depth: 4, say: ["Writing it up", "Wrote it up"] },
	generate_image: { noun: "an image", depth: 4, say: ["Making an image", "Made an image"] },
	// Drawn in the reply; listed here only when a call failed.
	show: { noun: "a figure", depth: 4, say: ["Drawing a figure", "Drew a figure"] },
	publish_to_github: {
		noun: "a page",
		depth: 4,
		say: (input, live) => {
			const repo = (input.repo as string) || "GitHub";
			return `${live ? "Publishing to" : "Published to"} ${repo}`;
		},
	},
	update_memory: {
		noun: "memory",
		depth: 4,
		say: ["Noting something to remember", "Noted something to remember"],
	},
	propose_narrative_identity_edit: {
		noun: "how you're described",
		depth: 4,
		say: [
			"Suggesting a change to how you're described",
			"Suggested a change to how you're described",
		],
	},
	get_project_item: {
		noun: "a project",
		depth: 4,
		say: ["Opening something you're working on", "Opened something you're working on"],
	},
	record_introductions: {
		noun: "the introductions",
		depth: 4,
		say: ["Writing the introductions", "Wrote the introductions"],
	},
	skip_step: { noun: "setup", depth: 4, say: ["Skipping a setup step", "Skipped a setup step"] },
	list_applets: { noun: "applets", depth: 4, say: ["Checking what's set up", "Checked what's set up"] },
	get_applet: { noun: "an applet", depth: 4, say: ["Checking what's set up", "Checked what's set up"] },
	setup_applet: {
		noun: "an applet",
		depth: 4,
		// `{guide: true}` only reads the authoring guide; it sets nothing up.
		say: (input, live) =>
			input.guide === true
				? live
					? "Reading how applets work"
					: "Read how applets work"
				: live
					? "Setting up an applet"
					: "Set up an applet",
	},
	edit_applet: { noun: "an applet", depth: 4, say: ["Changing an applet", "Changed an applet"] },
	delete_applet: { noun: "an applet", depth: 4, say: ["Removing an applet", "Removed an applet"] },
	run_applet: { noun: "an applet", depth: 4, say: ["Running an applet", "Ran an applet"] },
	update_applet_memory: {
		noun: "an applet",
		depth: 4,
		say: ["Noting something for next time", "Noted something for next time"],
	},
	// Many passes at once, by definition: this one IS the fan-out.
	dispatch_subagents: {
		noun: "a parallel search",
		depth: 5,
		say: ["Searching several ways at once", "Searched several ways at once"],
	},
};

/** The registry's tool ids this table covers — for the drift test. */
export const PRESENTED_TOOLS = Object.keys(TOOLS);

/** A tool this client has no entry for: an older client on a newer box. */
function unknownWords(name: string): string {
	return name.replace(/_/g, " ");
}

export function toolNoun(name: string): string {
	return TOOLS[name]?.noun ?? unknownWords(name);
}

export function toolDepth(name: string): 3 | 4 | 5 {
	return TOOLS[name]?.depth ?? 4;
}

/**
 * One call as a line. `live` picks the tense; `short` is the header's
 * one-line label, where a model-written argument is cut to fit — the list
 * below it passes the whole thing.
 */
export function describeTool(part: ToolPart, live: boolean, short = false): string {
	const name = toolName(part);
	const say = TOOLS[name]?.say;
	if (!say) {
		const words = unknownWords(name);
		return words.charAt(0).toUpperCase() + words.slice(1);
	}
	if (typeof say === "function") return say(part.input ?? {}, live, short);
	return live ? say[0] : say[1];
}

/** `data_communication_message` -> `messages`. The prefixes are our namespaces. */
function plainTableName(table: string): string {
	return table
		.replace(/^(data|wiki|narrative)_/, "")
		.replace(/^(communication|health|financial|activity|content)_/, "")
		.replace(/_/g, " ");
}

/** The header's budget for a model-written argument. */
const LABEL_ARG_CHARS = 40;

function fit(text: string, short: boolean): string {
	return short ? clip(text) : text;
}

/** Cut on a word boundary, with an ellipsis. */
export function clip(text: string, max = LABEL_ARG_CHARS): string {
	const flat = text.replace(/\s+/g, " ").trim();
	if (flat.length <= max) return flat;
	const cut = flat.slice(0, max);
	const space = cut.lastIndexOf(" ");
	return `${(space > max / 2 ? cut.slice(0, space) : cut).replace(/[\s,.;:]+$/, "")}…`;
}
