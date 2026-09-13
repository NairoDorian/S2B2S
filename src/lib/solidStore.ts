import { createRoot, createStore, type Store } from "solid-js";

/**
 * A `set` update: either a partial to shallow-merge into the store, or a
 * function of the current state that returns the partial to merge.
 *
 * This is the shape zustand's `set` had, and it is kept because the stores in
 * `src/stores/` were written against it. What changed underneath is the
 * engine: these are now Solid stores, not zustand stores, so the app can be
 * rendered by Solid without React's hook dispatcher in the loop.
 */
export type StoreUpdate<T> = Partial<T> | ((state: T) => Partial<T> | void);

export type StoreSet<T> = (update: StoreUpdate<T>) => void;

/**
 * Create a module-level Solid store with zustand's `set`/`get` ergonomics.
 *
 * Why this exists. `docs/PLAN_SOLIDJS_2.md` (D5) converts the nine zustand
 * stores to `createStore`. The stores carry real logic — `settingsStore` alone
 * is 815 lines of async IPC sequencing — and rewriting every `set(...)` /
 * `get()` call inside those bodies into draft mutations would have been a
 * large, mechanical, easy-to-get-subtly-wrong diff with no behavioural upside.
 * This helper supplies the two functions the bodies already call, on top of
 * Solid's own `createStore`, so the bodies are preserved verbatim:
 *
 *   - `set(partial)`    → `setState(draft => Object.assign(draft, partial))`
 *   - `set(fn)`         → the updater reads the draft and its result is merged
 *   - `get()`           → the store proxy itself
 *
 * The store is created inside a detached `createRoot` so it has an owner for
 * the lifetime of the module. These are app-lifetime singletons — the app has
 * no server renderer, so the "one module instance is shared across requests"
 * caveat in the Solid docs does not apply.
 */
export function createSolidStore<T extends object>(
  initializer: (set: StoreSet<T>, get: () => T) => T,
): Store<T> {
  let state!: Store<T>;
  let commit!: (fn: (draft: T) => void) => void;

  const get = () => state;
  const set: StoreSet<T> = (update) => {
    commit((draft) => {
      const partial =
        typeof update === "function"
          ? (update as (current: T) => Partial<T> | void)(draft as T)
          : update;
      if (partial) Object.assign(draft, partial);
    });
  };

  createRoot(() => {
    // The initial object is built **before** the store exists and handed to
    // `createStore` as its initial value, rather than being written in
    // afterwards. Two reasons, both load-bearing:
    //
    //   1. `initializer` only *defines* actions — it never calls `get()` or
    //      `set` while building the object — so the store it closes over does
    //      not have to exist yet.
    //   2. A write made after `createStore` would be **staged**: Solid commits
    //      store writes on the next microtask flush, so between module
    //      evaluation and that flush the store would read back empty. Any
    //      module-scope consumer (`main.tsx` calling `initialize()`) would then
    //      see a store with no actions on it. The initial value has no such
    //      delay.
    const initial = initializer(set, get);
    // `as never` because `createStore`'s overloads reject a bare `T` that could
    // itself be callable.
    const [store, setStore] = createStore<T>(initial as never);
    state = store;
    commit = setStore as unknown as (fn: (draft: T) => void) => void;
  });

  return state;
}
