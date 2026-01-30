# Sidebar Drag & Drop UX Specification

## Overview

This document defines the expected behavior for drag-and-drop interactions in the sidebar file/folder navigation. The goal is to match the intuitive UX patterns found in IDEs (VS Code, JetBrains), file managers (Finder, Explorer), and productivity tools (Notion).

---

## Core Principles

1. **Never lose data**: Items should never disappear. All operations must be atomic.
2. **Clear visual feedback**: Users should always know what will happen before they drop.
3. **Forgiving interactions**: Common mistakes should be easily recoverable.
4. **Familiar patterns**: Match established conventions from IDEs and file managers.

---

## Drag Behaviors

### Starting a Drag

| User Action | Expected Behavior |
|-------------|-------------------|
| Click + hold (150ms) on item | Drag starts; item becomes semi-transparent (opacity: 0.5) |
| Click + hold on folder header | Drag the entire folder (including contents) |
| Click + hold on folder chevron | No drag; toggle expand/collapse instead |

### During Drag

| User Action | Expected Behavior |
|-------------|-------------------|
| Move cursor | Dragged item follows cursor with smooth animation |
| Hover over drop zone | Zone highlights with accent border |
| Hover over closed folder for 500ms | Folder auto-expands (hover-to-expand) |
| Hover outside valid drop zones | Cursor changes to "not-allowed"; no highlight |
| Press Escape | Cancel drag; item returns to original position |

### Drop Targets

| Target | Visual Indicator | Result |
|--------|------------------|--------|
| Between items (root level) | Horizontal line indicator | Item moves to that position in workspace root |
| Between items (in folder) | Horizontal line indicator | Item moves to that position in folder |
| On folder header | Folder header highlights | Item moves INTO folder (at end) |
| On collapsed folder | Folder header highlights + expands | Item moves INTO folder |
| Empty workspace area | Subtle highlight | Item moves to workspace root (at end) |
| Different workspace (during swipe) | Blocked | Cannot drop during workspace transition |

---

## Drop Behaviors

### Drag Semantics by Source Type

The operation type (copy vs move) depends on where the item is dragged FROM:

| Source | Destination | Operation | Rationale |
|--------|-------------|-----------|-----------|
| **Smart view** | Root | **Copy** | Smart views are filters; item stays in view via filter match |
| **Smart view** | Manual folder | **Copy** | Item added to folder, still appears in smart view |
| **Smart view** | Smart view | **Blocked** | Cannot drop into smart views |
| **Manual folder** | Root | **Move** | Remove from folder, add to root |
| **Manual folder** | Manual folder | **Move** | Remove from source, add to destination |
| **Manual folder** | Smart view | **Blocked** | Cannot drop into smart views |
| **Root** | Manual folder | **Move** | Remove from root, add to folder |
| **Root** | Smart view | **Blocked** | Cannot drop into smart views |
| **Root** | Root | **Reorder** | Same zone, just reorder |
| **Same folder** | Same folder | **Reorder** | Same zone, just reorder |

**Key insight**: Dragging FROM a smart view is always a **copy** because smart views don't "contain" items—they display items matching a filter. The item continues to match the filter after being copied elsewhere.

### Move Operations (Manual Folders & Root)

| From | To | Behavior |
|------|----|---------|
| Workspace root | Folder | Add to folder, remove from root |
| Folder A | Folder B | Add to B, remove from A |
| Folder | Workspace root | Add to root, remove from folder |
| Same folder | Same folder | Reorder only |
| Same root | Same root | Reorder only |

### Operation Order (Critical for Data Safety)

```
1. Validate drop target is valid
2. Add item to destination (API call)
3. Wait for confirmation
4. Remove item from source (API call)
5. Update local state
6. Invalidate caches
```

**Important**: If step 2 fails, abort. If step 4 fails, show error but item exists in destination (recoverable).

---

## Smart View Behavior

Smart views are **filtered views** that dynamically display items matching certain criteria (e.g., all chats, recent pages). They have special behaviors for both auto-capture and drag operations.

### How Smart Views Work

Smart views automatically display items matching their filter criteria. No configuration needed.

```
User creates new chat →
  1. Chat is created
  2. Smart view "My Chats" (filter: namespace=chat) automatically includes it
  3. User sees the chat in the smart view
```

**Multiple matching smart views**: Items appear in ALL smart views whose filters they match. This is automatic—smart views are just live queries.

**Want an item at root too?** Drag it from the smart view to workspace root (this copies it, keeping it in both places).

### Dragging FROM Smart Views

Since smart views are read-only filtered views:

- **Drag OUT = Copy**: Dragging an item from a smart view to root/folder COPIES it (adds to destination)
- **Item stays in smart view**: Because the item still matches the filter
- **Use case**: "Pin" a favorite chat to root for quick access while keeping it organized in the smart view

```
Example:
1. Smart view "My Chats" shows all chats (filter: namespace=chat)
2. User drags "Project Discussion" from smart view to workspace root
3. "Project Discussion" now appears in BOTH:
   - Workspace root (explicitly added)
   - "My Chats" smart view (still matches filter)
```

### Dragging INTO Smart Views

- **Blocked**: Cannot drop items into smart views
- **Rationale**: Smart views are defined by filters, not manual curation
- **Visual feedback**: "not-allowed" cursor when hovering over smart view during drag

### Smart View vs Manual Folder Summary

| Aspect | Smart View | Manual Folder |
|--------|------------|---------------|
| Contents defined by | Filter criteria (automatic) | Manual drag/drop |
| Drag INTO | Blocked | Allowed |
| Drag FROM | Copy (item stays) | Move (item removed) |
| Item ordering | By filter sort (e.g., recent first) | Manual reorder via drag |

---

## Visual Feedback States

### Item States

```css
/* Default */
.sidebar-item { opacity: 1; }

/* Being dragged */
.sidebar-item[aria-grabbed="true"] {
  opacity: 0.5;
  transform: scale(1.02);
  box-shadow: 0 4px 12px rgba(0,0,0,0.15);
}

/* Valid drop target */
.sidebar-item.drop-target {
  background: var(--color-primary-subtle);
  outline: 2px solid var(--color-primary);
}

/* Hover-to-expand pending */
.folder.expand-pending {
  background: var(--color-primary-subtle);
  animation: pulse 1s ease-in-out;
}
```

### Drop Indicators

```css
/* Line indicator between items */
.drop-indicator-line {
  height: 2px;
  background: var(--color-primary);
  border-radius: 1px;
  margin: 0 8px;
}

/* Folder drop highlight */
.folder-drop-highlight {
  background: color-mix(in srgb, var(--color-primary) 15%, transparent);
  border-radius: 4px;
}
```

---

## Hover-to-Expand Behavior

When dragging an item over a **collapsed folder**:

```
Timeline:
0ms    - Hover begins
100ms  - Folder header highlights (visual feedback)
500ms  - Folder auto-expands with animation
        - User can now drop INTO folder contents
```

### Implementation Notes

```typescript
// Hover-to-expand state per folder
let hoverExpandTimeout: ReturnType<typeof setTimeout> | null = null;
let isExpandPending = $state(false);

function handleDragEnter(e: DragEvent) {
  if (!isExpanded) {
    isExpandPending = true;
    hoverExpandTimeout = setTimeout(() => {
      isExpanded = true;
      isExpandPending = false;
    }, 500);
  }
}

function handleDragLeave(e: DragEvent) {
  if (hoverExpandTimeout) {
    clearTimeout(hoverExpandTimeout);
    hoverExpandTimeout = null;
  }
  isExpandPending = false;
}
```

---

## Edge Cases & Error Handling

### Race Condition Prevention

**Problem**: Dragging item out of folder causes it to disappear.

**Cause**: Remove fires before add completes, or cache invalidation happens mid-operation.

**Solution**:

```typescript
async function handleDrop(item, destination) {
  // 1. Optimistically update UI
  const rollbackState = captureState();

  try {
    // 2. Add to destination FIRST (item now exists in both places momentarily)
    await addToDestination(item, destination);

    // 3. Remove from source only after add succeeds
    await removeFromSource(item);

    // 4. Single cache invalidation AFTER both operations
    invalidateCache();
  } catch (error) {
    // Rollback on any failure
    restoreState(rollbackState);
    showErrorToast("Failed to move item. Please try again.");
  }
}
```

### Invalid Drop Targets

| Scenario | Behavior |
|----------|----------|
| Drop folder into itself | Blocked; cursor shows "not-allowed" |
| Drop item into its current folder | Treat as reorder (no-op if same position) |
| Drop during network error | Show error; item stays in original position |
| Drop into smart view | Blocked; smart views are filter-based, not manually curated |
| Drop into system folder | Blocked; system folders are read-only |
| Drop folder exceeding max depth | Blocked; max depth is folder > folder > files (2 levels) |

### Keyboard Support

| Key | During Drag |
|-----|-------------|
| Escape | Cancel drag, return item to origin |

> **Note**: Full keyboard navigation during drag (Tab, Arrow keys, Enter) was considered but deemed too complex for the value it provides. Escape-to-cancel covers the primary use case.

---

## Animation Specifications

### Drag Start

```css
transition: opacity 150ms ease-out, transform 150ms ease-out;
```

### Reorder (FLIP animation)

```css
transition: transform 200ms cubic-bezier(0.2, 0, 0, 1);
```

### Folder Expand

```css
transition: max-height 200ms ease-out, opacity 150ms ease-out;
```

### Drop Indicator

```css
transition: opacity 100ms ease-out, transform 100ms ease-out;
```

---

## State Machine

```
                    ┌─────────────┐
                    │    IDLE     │
                    └──────┬──────┘
                           │ mousedown + 150ms
                           ▼
                    ┌─────────────┐
            ┌───────│  DRAGGING   │───────┐
            │       └──────┬──────┘       │
            │              │              │
     hover folder     hover zone     drop/escape
            │              │              │
            ▼              ▼              ▼
    ┌───────────────┐ ┌─────────┐  ┌─────────────┐
    │ EXPAND_PENDING│ │ OVER_   │  │  DROPPED/   │
    │   (500ms)     │ │ TARGET  │  │  CANCELLED  │
    └───────┬───────┘ └────┬────┘  └──────┬──────┘
            │              │              │
            └──────────────┴──────────────┘
                           │
                           ▼
                    ┌─────────────┐
                    │    IDLE     │
                    └─────────────┘
```

---

## Testing Checklist

### Basic Operations

- [ ] Drag item within same folder (reorder)
- [ ] Drag item from folder to workspace root
- [ ] Drag item from workspace root to folder
- [ ] Drag item between two different folders
- [ ] Drag folder within workspace root (reorder)

### Smart View Operations

- [ ] Drag item FROM smart view to workspace root (copy, item stays in smart view)
- [ ] Drag item FROM smart view to manual folder (copy, item stays in smart view)
- [ ] Attempt drag INTO smart view (should be blocked, "not-allowed" cursor)
- [ ] New item automatically appears in matching smart view (filter works)

### Edge Cases

- [ ] Drag item to closed folder (should expand after 500ms)
- [ ] Drag item and press Escape (should cancel)
- [ ] Drag during slow network (should show loading state)
- [ ] Drag item to current location (should be no-op)
- [ ] Rapid drag-drop operations (no race conditions)
- [ ] Drag folder into folder at max depth (should be blocked)

### Visual Feedback

- [ ] Drop indicator line appears between items
- [ ] Folder highlights when valid drop target
- [ ] Invalid targets show "not-allowed" cursor
- [ ] Dragged item has reduced opacity

### Accessibility

- [ ] Escape key cancels drag operation
- [ ] Screen reader announces drag state changes

---

## Implementation Priority

### Phase 1: Fix Critical Bugs

1. Fix "disappearing items" race condition (operation ordering)
2. Add proper error handling with rollback
3. Ensure cache invalidation happens atomically

### Phase 2: Core UX Improvements

1. Add hover-to-expand for closed folders
2. Add clear drop indicator lines
3. Add folder header highlight for "drop into" feedback
4. Implement copy semantics for smart view drags (vs move for manual folders)
5. Block drops into smart views with visual feedback

### Phase 3: Polish

1. Fine-tune animations and timing
2. Block folder nesting beyond max depth (2 levels)
3. Add screen reader announcements for drag state changes

---

## URL & Link Import

The sidebar is **URL-based** - items are stored as URLs and resolved to display metadata. This enables importing links from various sources.

### Known vs Unknown (It's Just About Icons)

Since the sidebar stores URLs and resolves metadata, the only real difference is display quality:

| Category | Resolution | Result |
|----------|------------|--------|
| **Known** | Full entity lookup | Rich metadata + specific icon |
| **Unknown** | Best-effort fetch | Fallback name + 🔗 icon |

**Known namespaces** (10 total):
`chat`, `page`, `person`, `place`, `org`, `thing`, `day`, `year`, `source`, `drive`

**Unknown** = Any other URL. External URLs get OpenGraph/favicon fetch. Internal unknowns use path as name.

### Import Methods

**1. Drag from external source (browser, other app)**

```
User drags URL → drops on sidebar
→ URL added at drop position
→ Backend attempts resolution (known lookup or best-effort fetch)
→ Item appears with resolved/fallback metadata
```

**2. Drag from Drive view (internal cross-view)**

```
User drags file from Drive tab → drops on sidebar
→ Drive URL (/drive/file_xyz) added at drop position
→ Resolves as known entity
```

**3. Paste (Cmd+V) - Future**

```
User copies URL, focuses sidebar, pastes
→ URL added at end of focused zone
```

### Backend Resolution

```
resolveUrl(url) →
  IF matches known namespace (/page/, /chat/, /drive/, etc.)
    → Query entity/file table → return full metadata
  ELSE (unknown)
    → IF external URL (https://, http://)
        → Fetch OpenGraph/title + favicon (cached)
    → ELSE
        → Use URL path as display name
    → Return { name, icon, namespace: "link" }
```

### Visual Distinction

| Category | Icon | Indicator |
|----------|------|-----------|
| Known | Entity-specific icon | None |
| Unknown (external) | Favicon or 🔗 | Small ↗ badge |
| Unknown (internal) | 🔗 | None |

### Testing Checklist (URL Import)

- [ ] Drag external URL from browser to sidebar
- [ ] Drag Drive file from Drive view to sidebar
- [ ] Unknown URL shows best-effort title/favicon
- [ ] Click known item opens in app tab
- [ ] Click external URL opens new browser tab

---

## Related Files

- [UnifiedSidebar.svelte](./UnifiedSidebar.svelte) - Workspace root DnD
- [UnifiedFolder.svelte](./UnifiedFolder.svelte) - Folder content DnD
- [dndManager.svelte.ts](../../stores/dndManager.svelte.ts) - DnD state management
- [sidebar.css](../../styles/sidebar.css) - DnD visual styles
