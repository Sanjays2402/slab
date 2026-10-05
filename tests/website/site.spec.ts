import { test, expect } from "@playwright/test";

test("feature filters, search and empty-state recovery work with keyboard controls", async ({ page }) => {
  const errors:string[]=[]; page.on("pageerror",e=>errors.push(e.message));
  expect((await page.goto("./"))!.status()).toBe(200);
  const cards=page.locator("[data-feature-category]:visible");
  await expect(cards).toHaveCount(18);
  for(const name of ["Organize","Convert","Protect","Automate","Review","Create"]) {
    const button=page.getByRole("button",{name,exact:true}); await button.click();
    await expect(cards).toHaveCount(3); await expect(button).toHaveAttribute("aria-pressed","true");
  }
  await page.getByRole("button",{name:"All features",exact:true}).click();
  const search=page.getByRole("searchbox",{name:"Find a feature",exact:true});
  await search.fill("OCR"); await expect(cards).toHaveCount(1);
  await search.fill("Word Excel"); await expect(cards).toHaveCount(1);
  await page.getByRole("button",{name:"Protect",exact:true}).click(); await expect(cards).toHaveCount(0);
  await expect(page.locator("#featureEmpty")).toBeVisible();
  await page.getByRole("button",{name:"Show all features",exact:true}).click();
  await expect(cards).toHaveCount(18); await expect(search).toHaveValue(""); await expect(search).toBeFocused();
  await page.getByRole("button",{name:"Convert",exact:true}).focus(); await page.keyboard.press("Enter"); await expect(cards).toHaveCount(3);
  expect(errors).toEqual([]);
});

test("mobile navigation, seven viewport sizes and JavaScript-disabled content work", async ({ page,browser }) => {
  await page.goto("./");
  for(const width of [1440,1024,1000,900,768,390,320]) {
    await page.setViewportSize({width,height:1000});
    expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth),`overflow at ${width}`).toBe(true);
  }
  await page.setViewportSize({width:390,height:1000});
  const menu=page.locator("#menuBtn");
  await menu.click(); await expect(menu).toHaveAttribute("aria-expanded","true");
  await page.locator("#mobileLinks").getByRole("link",{name:"Features",exact:true}).click(); await expect(menu).toHaveAttribute("aria-expanded","false");
  const nojs=await browser.newPage({javaScriptEnabled:false,viewport:{width:390,height:1000}});
  try {
    await nojs.goto(test.info().project.use.baseURL!);
    await expect(nojs.locator("[data-feature-category]:visible")).toHaveCount(18);
    await expect(nojs.locator("#featureControls")).toBeHidden();
    expect(await nojs.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
  } finally { await nojs.close(); }
});

test("screenshots, internal links, policy pages and release labels resolve", async ({ page,request }) => {
  await page.goto("./");
  const images=page.locator("img");
  for(let i=0;i<await images.count();i++) {
    const image=images.nth(i); await image.scrollIntoViewIfNeeded();
    await expect.poll(()=>image.evaluate((el:HTMLImageElement)=>el.complete && el.naturalWidth>0)).toBe(true);
  }
  for(const href of await page.locator('a[href^="#"]').evaluateAll(els=>els.map(el=>el.getAttribute("href")!))) {
    await expect(page.locator(href)).toHaveCount(1);
  }
  for(const path of ["privacy.html","terms.html","fair-use.html"]) {
    expect((await request.get(path)).status(),path).toBe(200);
    await page.goto(path); await expect(page.locator("h1")).not.toBeEmpty();
    expect(await page.evaluate(()=>document.documentElement.scrollWidth<=innerWidth)).toBe(true);
  }
  await page.goto("./");
  await expect(page.locator('#coming-next')).toContainText("current download is v3.40.0");
  await expect(page.locator('a[href*="/releases/download/v3.40.0/"]')).toHaveCount(7);
});
