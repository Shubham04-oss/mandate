import { type Context, type ContextManager, ROOT_CONTEXT } from "@opentelemetry/api";

// The smallest context manager that makes `context.with` set the active span (no SDK in tests).
export class StackContextManager implements ContextManager {
  #active: Context = ROOT_CONTEXT;
  active() {
    return this.#active;
  }
  with<A extends unknown[], F extends (...args: A) => ReturnType<F>>(
    ctx: Context,
    fn: F,
    thisArg?: ThisParameterType<F>,
    ...args: A
  ): ReturnType<F> {
    const prev = this.#active;
    this.#active = ctx;
    try {
      return fn.call(thisArg, ...args);
    } finally {
      this.#active = prev;
    }
  }
  bind<T>(_ctx: Context, target: T): T {
    return target;
  }
  enable() {
    return this;
  }
  disable() {
    return this;
  }
}
