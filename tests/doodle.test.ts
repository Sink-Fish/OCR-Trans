import { test } from "node:test";
import assert from "node:assert/strict";
import { finishShape, translate, type Shape } from "../src/lib/doodle.ts";

test("annotation commit includes the pointer-up position even without a final move event", () => {
  const draft: Shape = { kind: "rect", color: "#000000", width: 2, x0: 10, y0: 20, x1: 10, y1: 20 };
  assert.deepEqual(finishShape(draft, 30, 40), { ...draft, x1: 30, y1: 40 });
  assert.equal(draft.x1, 10, "preserve the pre-drag state for undo");
  const pen: Shape = { kind: "pen", color: "#000000", width: 2, points: [[1, 2]] };
  const ended = finishShape(pen, 3, 4);
  assert.deepEqual(ended, { ...pen, points: [[1, 2], [3, 4]] });
  assert.deepEqual(finishShape(ended, 3, 4), ended, "no repeated final point");
});

test("moving a selected annotation uses its original shape and final displacement", () => {
  const pen: Shape = { kind: "pen", color: "#000000", width: 2, points: [[1, 2], [3, 4]] };
  assert.deepEqual(translate(pen, 20, -10), { ...pen, points: [[21, -8], [23, -6]] });
  assert.deepEqual(pen.points, [[1, 2], [3, 4]], "original remains available for cancellation and undo");
});
