// FCB-022 consumer verification document: TypeScript route.
// Exercises: type keywords, interface/type aliases, generics vs comparisons,
// template literal types, type assertions (as/satisfies), type predicates (is/asserts),
// access modifiers, enums, namespaces, and composed JavaScript constructs.

import { SessionManager } from "./session.js";

export type Identifier = string | number;

export interface Resource<T> {
  readonly id: Identifier;
  data: T;
  metadata?: Record<string, unknown>;
}

export enum StatusCode {
  Ok = 200,
  NotFound = 404,
  InternalError = 500,
}

export abstract class BaseService<T> {
  public readonly name: string;
  private retryCount: number = 0;
  protected active: boolean = false;

  constructor(name: string) {
    this.name = name;
  }

  public abstract execute(): Promise<T>;

  public updateRetry(count: number): void {
    this.retryCount = count;
    // Comparison operators must not be confused with generic angle brackets
    if (this.retryCount < 5 && count > 0) {
      this.active = true;
    }
  }
}

// Template literal types
export type EventName = `on${string}`;
export type Nullable<T> = T | null | undefined;

// Type predicate and assertions
export function isString(val: unknown): val is string {
  return typeof val === "string";
}

export function parseData<T>(input: unknown): T {
  const raw = (input as { payload: unknown }).payload;
  return raw as T;
}

// Satisfies expression and template literal with interpolation
export const config = {
  host: "127.0.0.1",
  port: 8080,
  prefix: "/api/v1",
} satisfies Record<string, unknown>;

export function formatBanner<T>(resource: Resource<T>): string {
  const ratio = 100 / 2 / 5;
  const pattern = /^[a-z0-9_-]+$/i;
  if (pattern.test(String(resource.id)) && ratio > 0) {
    return `Resource [${resource.id}]: ${JSON.stringify(resource.data)}`;
  }
  return "invalid";
}
