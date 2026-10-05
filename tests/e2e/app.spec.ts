import { test, expect } from "@playwright/test";
import { PDFDocument } from "pdf-lib";
import { readFileSync } from "node:fs";

test.beforeEach(async ({ page }) => {
  await page.addInitScript(() => { if (!localStorage.getItem("slab.ui.config.v1")) localStorage.setItem("slab.ui.config.v1", JSON.stringify({onboarded:true,theme:"dark",accent:"orange",density:"comfortable"})); });
});

test("every desktop navigation panel mounts without uncaught errors", async ({ page }) => {
  const errors: string[]=[]; page.on("pageerror",error=>errors.push(error.message));
  await page.goto("/");
  const buttons=page.locator('nav[aria-label="Primary"] .nav-item');
  await expect(buttons.first()).toBeVisible({timeout:20000});
  const count=await buttons.count(); expect(count).toBeGreaterThan(60);
  for(let i=0;i<count;i++) {
    await buttons.nth(i).click();
    await expect(buttons.nth(i)).toHaveAttribute("aria-current","page");
    await expect(page.locator("main.content")).not.toBeEmpty();
    // The Forms hub intentionally opens a first-visit tour.
    await page.keyboard.press("Escape");
  }
  expect(errors).toEqual([]);
});

test("Glass and accents persist, and Settings works on a narrow window", async ({ page }) => {
  await page.goto("/"); await page.getByRole("button",{name:"Settings",exact:true}).click();
  await page.getByRole("radio",{name:"Glass",exact:true}).click();
  await page.getByRole("radio",{name:"Teal",exact:true}).click();
  await expect(page.locator("html")).toHaveAttribute("data-theme","glass");
  await page.reload();
  await expect(page.locator("html")).toHaveAttribute("data-accent","teal");
  await page.getByRole("button",{name:"Settings",exact:true}).click();
  await page.setViewportSize({width:800,height:700});
  expect(await page.locator("main").evaluate(el=>el.scrollWidth<=el.clientWidth)).toBe(true);
});

test("Merge rejects malformed ranges before opening the save dialog", async ({ page }) => {
  await page.addInitScript(() => {
    const state=window as unknown as {__TAURI_INTERNALS__: unknown; saveCalls: number};
    state.saveCalls=0;
    state.__TAURI_INTERNALS__={invoke:async (cmd:string) => {
      if(cmd==="plugin:dialog|open") return ["/tmp/first.pdf","/tmp/second.pdf"];
      if(cmd==="plugin:dialog|save") { state.saveCalls++; return null; }
      if(cmd==="slab_ui_config_read") return {kind:"ok",value:{onboarded:true}};
      if(cmd==="slab_first_launch_probe") return {kind:"ok",value:{should_prompt:false}};
      if(cmd.startsWith("slab_plugins_active_") || cmd.includes("list")) return [];
      return null;
    },transformCallback:()=>0,unregisterCallback:()=>{}};
  });
  await page.goto("/"); await page.locator('nav[aria-label="Primary"]').getByRole("button",{name:"Merge",exact:true}).click();
  await page.getByRole("button",{name:/Choose PDFs to merge/}).click();
  for(const value of ["1oops-3","1.5-3",",,","4294967296"]) {
    await page.getByRole("textbox",{name:"Pages to take from first.pdf",exact:true}).fill(value);
    await page.getByRole("button",{name:"Merge 2 PDFs",exact:true}).click();
    await expect(page.locator(".status.err")).toContainText("bad page range");
  }
  expect(await page.evaluate(()=>(window as unknown as {saveCalls:number}).saveCalls)).toBe(0);
});

test("browser samples load, page edits download a valid PDF, and metadata round-trips", async ({ page }) => {
  const errors: string[]=[]; page.on("pageerror",error=>errors.push(error.message));
  await page.goto("/try/pages?sample=employment-offer");
  await expect(page.locator(".thumb img")).toHaveCount(2);
  await expect(page.getByRole("button",{name:"Save as PDF",exact:true})).toBeEnabled();
  await page.locator(".thumb").first().click();
  await page.getByRole("button",{name:"Rotate",exact:false}).click();
  await expect(page.locator(".toolbar .status")).toContainText("Rotated 1");
  const downloadPromise=page.waitForEvent("download");
  await page.getByRole("button",{name:"Save as PDF",exact:true}).click();
  const download=await downloadPromise;
  const doc=await PDFDocument.load(readFileSync((await download.path())!));
  expect(doc.getPageCount()).toBe(2); expect(doc.getPage(0).getRotation().angle).toBe(90);
  await page.goto("/try/metadata?sample=employment-offer");
  await expect(page.getByLabel("Author",{exact:true})).not.toHaveValue("");
  await page.getByLabel("Title",{exact:true}).fill("E2E updated title");
  const metaPromise=page.waitForEvent("download");
  await page.getByRole("button",{name:"Save as new PDF",exact:true}).click();
  const meta=await metaPromise;
  expect((await PDFDocument.load(readFileSync((await meta.path())!))).getTitle()).toBe("E2E updated title");
  expect(errors).toEqual([]);
});

test("Markdown creates a downloadable PDF and every browser navigation link resolves", async ({ page,request }) => {
  await page.goto("/try/markdown");
  await page.getByRole("textbox",{name:"Markdown editor",exact:true}).fill("# End to end\n\nA real PDF, created in the browser.");
  await expect(page.getByRole("button",{name:"Download PDF",exact:false})).toBeEnabled();
  const promise=page.waitForEvent("download"); await page.getByRole("button",{name:"Download PDF",exact:false}).click();
  const download=await promise; expect((await PDFDocument.load(readFileSync((await download.path())!))).getPageCount()).toBeGreaterThan(0);
  for(const link of await page.locator(".try-nav a[href^='/']").evaluateAll(els=>els.map(el=>el.getAttribute("href")!))) {
    expect((await request.get(link)).status(),link).toBe(200);
  }
});


test("OCR validates language and handles save cancellation, success and backend errors", async ({ page }) => {
  await page.addInitScript(() => {
    const state=window as unknown as {__TAURI_INTERNALS__:unknown; ocrCalls:unknown[]; output:string|null; ocrError:boolean};
    state.ocrCalls=[]; state.output=null; state.ocrError=false;
    state.__TAURI_INTERNALS__={invoke:async (cmd:string,args:unknown) => {
      if(cmd==="plugin:dialog|open") return "/tmp/scan.pdf";
      if(cmd==="plugin:dialog|save") return state.output;
      if(cmd==="slab_ocr") { state.ocrCalls.push(args); return state.ocrError ? {kind:"err",message:"Tesseract language not installed"} : {kind:"ok",value:{pages:2}}; }
      if(cmd==="slab_ui_config_read") return {kind:"ok",value:{onboarded:true}};
      if(cmd==="slab_first_launch_probe") return {kind:"ok",value:{should_prompt:false}};
      if(cmd.startsWith("slab_plugins_active_") || cmd.includes("list")) return [];
      return null;
    },transformCallback:()=>0,unregisterCallback:()=>{}};
  });
  await page.goto("/"); await page.locator('nav[aria-label="Primary"]').getByRole("button",{name:"OCR",exact:true}).click();
  await page.getByRole("button",{name:/Choose a scanned PDF/}).click();
  const run=page.getByRole("button",{name:"Create searchable PDF",exact:true});
  await page.getByLabel("OCR language",{exact:true}).fill("../eng"); await expect(run).toBeDisabled();
  await page.getByLabel("OCR language",{exact:true}).fill("eng+fra");
  await run.click(); await expect(run).toBeEnabled();
  expect(await page.evaluate(()=>(window as unknown as {ocrCalls:unknown[]}).ocrCalls)).toEqual([]);
  await page.evaluate(()=>{(window as unknown as {output:string}).output="/tmp/scan.pdf";});
  await run.click(); await expect(page.locator(".status.err")).toContainText("Choose a new output file");
  await page.evaluate(()=>{(window as unknown as {output:string}).output="/tmp/scan-ocr.pdf";});
  await run.click(); await expect(page.locator(".saved-files")).toContainText("1 PDF saved");
  expect(await page.evaluate(()=>(window as unknown as {ocrCalls:unknown[]}).ocrCalls)).toEqual([{input:"/tmp/scan.pdf",output:"/tmp/scan-ocr.pdf",opts:{lang:"eng+fra",dpi:300}}]);
  await page.evaluate(()=>{(window as unknown as {ocrError:boolean}).ocrError=true;});
  await run.click(); await expect(page.locator(".status.err")).toContainText("Tesseract language not installed");
  await expect(page.locator(".saved-files")).toHaveCount(0); await expect(run).toBeEnabled();
});

test("balanced Split previews and submits every page once, with recoverable save cancellation", async ({ page }) => {
  await page.addInitScript(() => {
    const state=window as unknown as {__TAURI_INTERNALS__:unknown; splitCalls:unknown[]; folder:string|null};
    state.splitCalls=[]; state.folder=null;
    state.__TAURI_INTERNALS__={invoke:async (cmd:string,args:any) => {
      if(cmd==="plugin:dialog|open") return args.options.directory ? state.folder : "/tmp/report.pdf";
      if(cmd==="slab_page_count") return {kind:"ok",value:7};
      if(cmd==="slab_split_ranges") { state.splitCalls.push(args); return {kind:"ok",value:["/tmp/output/report-1-1-3.pdf","/tmp/output/report-2-4-5.pdf","/tmp/output/report-3-6-7.pdf"]}; }
      if(cmd==="slab_ui_config_read") return {kind:"ok",value:{onboarded:true}};
      if(cmd==="slab_first_launch_probe") return {kind:"ok",value:{should_prompt:false}};
      if(cmd.startsWith("slab_plugins_active_") || cmd.includes("list")) return [];
      return null;
    },transformCallback:()=>0,unregisterCallback:()=>{}};
  });
  await page.goto("/"); await page.locator('nav[aria-label="Primary"]').getByRole("button",{name:"Split",exact:true}).click();
  await page.getByRole("button",{name:/Choose a PDF/}).click();
  await page.getByLabel("Number of output files",{exact:true}).fill("3");
  await expect(page.locator(".split-preview ol li")).toHaveCount(3);
  await expect(page.locator(".preview-summary")).toContainText("7 of 7 pages");
  const run=page.getByRole("button",{name:"Create 3 PDFs",exact:true});
  await run.click(); await expect(run).toBeEnabled();
  expect(await page.evaluate(()=>(window as unknown as {splitCalls:unknown[]}).splitCalls)).toEqual([]);
  await page.evaluate(()=>{(window as unknown as {folder:string}).folder="/tmp/output";});
  await run.click(); await expect(page.locator(".saved-files")).toContainText("3 PDFs saved");
  expect(await page.evaluate(()=>(window as unknown as {splitCalls:unknown[]}).splitCalls)).toEqual([{input:"/tmp/report.pdf",outDir:"/tmp/output",ranges:[{start:1,end:3},{start:4,end:5},{start:6,end:7}]}]);
  await page.getByLabel("Number of output files",{exact:true}).fill("8");
  await expect(page.getByRole("button",{name:"Split PDF",exact:true})).toBeDisabled();
});

test("browser demo navigation fits mobile screens", async ({ page }) => {
  for(const width of [390,320]) {
    await page.setViewportSize({width,height:800}); await page.goto("/try");
    await expect(page.locator(".try-nav")).toBeVisible();
    expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`overflow at ${width}`).toBe(true);
  }
});

for (const total of [7, 6, 1]) {
  test(`odd/even Split previews ${total} pages and handles cancellation, saving and failure`, async ({ page }) => {
    const errors: string[]=[]; page.on("pageerror", error=>errors.push(error.message));
    await page.addInitScript(({ total }) => {
      const state=window as unknown as {__TAURI_INTERNALS__:unknown; parityCalls:unknown[]; folder:string|null; fail:boolean};
      state.parityCalls=[]; state.folder=null; state.fail=false;
      state.__TAURI_INTERNALS__={invoke:async (cmd:string,args:any) => {
        if(cmd==="plugin:dialog|open") return args.options.directory ? state.folder : "/tmp/report.pdf";
        if(cmd==="slab_page_count") return {kind:"ok",value:total};
        if(cmd==="slab_split_odd_even") {
          state.parityCalls.push(args);
          return state.fail ? {kind:"err",message:"Output folder is read-only"} : {kind:"ok",value:total===1 ? ["/tmp/output/report-odd.pdf"] : ["/tmp/output/report-odd.pdf","/tmp/output/report-even.pdf"]};
        }
        if(cmd==="slab_split_ranges") throw new Error("Odd/even mode must use the grouped split command");
        if(cmd==="slab_ui_config_read") return {kind:"ok",value:{onboarded:true}};
        if(cmd==="slab_first_launch_probe") return {kind:"ok",value:{should_prompt:false}};
        if(cmd.startsWith("slab_plugins_active_") || cmd.includes("list")) return [];
        return null;
      },transformCallback:()=>0,unregisterCallback:()=>{}};
    }, { total });
    await page.goto("/");
    await page.locator('nav[aria-label="Primary"]').getByRole("button",{name:"Split",exact:true}).click();
    await page.getByRole("button",{name:/Choose a PDF/}).click();
    const tab=page.getByRole("button",{name:"Odd / even",exact:true});
    await tab.click(); await expect(tab).toHaveAttribute("aria-pressed","true");
    const count=total===1 ? 1 : 2;
    await expect(page.locator(".split-preview ol li")).toHaveCount(count);
    await expect(page.locator(".preview-summary")).toContainText(`${total} of ${total} pages`);
    await expect(page.locator(".split-preview ol li").first()).toContainText(`report-odd.pdf`);
    await expect(page.locator(".split-preview ol li").first()).toContainText(`${Math.ceil(total/2)} page`);
    if(total===7) await expect(page.locator(".split-preview ol li").first()).toContainText("1, 3, …, 7");
    if(total>1) await expect(page.locator(".split-preview ol li").last()).toContainText(`report-even.pdf`);
    const run=page.getByRole("button",{name:`Create ${count} PDF${count===1 ? "" : "s"}`,exact:true});
    await run.click(); await expect(run).toBeEnabled();
    expect(await page.evaluate(()=>(window as unknown as {parityCalls:unknown[]}).parityCalls)).toEqual([]);
    await page.evaluate(()=>{(window as unknown as {folder:string}).folder="/tmp/output";});
    await run.click(); await expect(page.locator(".saved-files")).toContainText(`${count} PDF${count===1 ? "" : "s"} saved`);
    expect(await page.evaluate(()=>(window as unknown as {parityCalls:unknown[]}).parityCalls)).toEqual([{input:"/tmp/report.pdf",outDir:"/tmp/output"}]);
    await page.evaluate(()=>{(window as unknown as {fail:boolean}).fail=true;});
    await run.click(); await expect(page.locator(".status.err")).toContainText("read-only");
    await expect(page.locator(".saved-files")).toHaveCount(0); await expect(run).toBeEnabled();
    await page.setViewportSize({width:800,height:700});
    expect(await page.locator("main.content").evaluate(el=>el.scrollWidth<=el.clientWidth)).toBe(true);
    expect(errors).toEqual([]);
  });
}
