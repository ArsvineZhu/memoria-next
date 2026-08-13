import { access } from "node:fs/promises";
import { resolve } from "node:path";

await access(resolve("dist", "index.js"));
console.log("packed-consumer entrypoint is present");
