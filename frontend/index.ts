import packageInfo from "package.json";

document.body.innerText = "hello";

console.log(packageInfo);

new Worker(new URL("./worker.ts", import.meta.url));
