/* tslint:disable */
/* eslint-disable */

/* auto-generated by NAPI-RS */

export function sum(a: number, b: number): number
export interface DbOptions {
  readonly: boolean
}
export function createDb(path: string, opts?: DbOptions | undefined | null): Database
export function get(db: Database, key: Buffer): Promise<unknown>
export interface IterationOption {
  limit: number
  reverse: boolean
  gte?: Buffer
  lte?: Buffer
}
export function getIterator(callback: (err: null | Error, result: { key: Buffer; value: Buffer}) => void): Promise<unknown>
export interface Kv {
  key: Buffer
  value: Buffer
}
export class Database { }
