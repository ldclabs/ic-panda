/** Convert only against a generated Candid type, never guess opt/vec or variant shapes. */
import { IDL } from "@icp-sdk/core/candid";
import { Principal } from "@icp-sdk/core/principal";
import { ensure } from "./cose-errors.ts";

export function fromCandid(type: IDL.Type, value: any): any {
  if (type instanceof IDL.RecClass) return fromCandid(type.getType()!, value);
  if (type instanceof IDL.OptClass) {
    ensure(Array.isArray(value) && value.length <= 1, "INVALID_INPUT");
    return value.length ? fromCandid(type._type, value[0]) : null;
  }
  if (type instanceof IDL.VecClass)
    return type._type.name === "nat8"
      ? Uint8Array.from(value)
      : Array.from(value, (v) => fromCandid(type._type, v));
  if (type instanceof IDL.TupleClass)
    return type._fields.map(([, t], i) => fromCandid(t, value[i]));
  if (type instanceof IDL.RecordClass)
    return Object.fromEntries(
      type._fields.map(([k, t]) => [k, fromCandid(t, value[k])]),
    );
  if (type instanceof IDL.VariantClass) {
    const keys = Object.keys(value);
    ensure(keys.length === 1, "INVALID_INPUT");
    const tag = keys[0]!;
    const child = type._fields.find(([k]) => k === tag)?.[1];
    ensure(child, "UNSUPPORTED_PROTOCOL");
    return child.name === "null"
      ? tag
      : { [tag]: fromCandid(child, value[tag]) };
  }
  if (type.name === "principal") return value.toUint8Array();
  if (/^(nat|int)/.test(type.name)) return BigInt(value);
  return value;
}
export function toCandid(type: IDL.Type, value: any): any {
  if (type instanceof IDL.RecClass) return toCandid(type.getType()!, value);
  if (type instanceof IDL.OptClass)
    return value === null ? [] : [toCandid(type._type, value)];
  if (type instanceof IDL.VecClass)
    return type._type.name === "nat8"
      ? Uint8Array.from(value)
      : value.map((v: any) => toCandid(type._type, v));
  if (type instanceof IDL.TupleClass)
    return type._fields.map(([, t], i) => toCandid(t, value[i]));
  if (type instanceof IDL.RecordClass)
    return Object.fromEntries(
      type._fields.map(([k, t]) => [k, toCandid(t, value[k])]),
    );
  if (type instanceof IDL.VariantClass) {
    const tag = typeof value === "string" ? value : Object.keys(value)[0]!;
    const child = type._fields.find(([k]) => k === tag)?.[1];
    ensure(child, "UNSUPPORTED_PROTOCOL");
    return {
      [tag]: child.name === "null" ? null : toCandid(child, value[tag]),
    };
  }
  if (type.name === "principal") return Principal.fromUint8Array(value);
  if (["nat8", "nat16", "nat32", "int8", "int16", "int32"].includes(type.name))
    return Number(value);
  return value;
}
