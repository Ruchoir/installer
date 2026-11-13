// The exit test of the installer v0: on a fresh instance, a person signs up on Propolis's pages,
// confirms their address with the code they received, signs in and reaches their account.
import { expect, test, type APIRequestContext } from "@playwright/test";

/** Where the development instance's mail catcher answers (Mailpit's API). */
const MAILPIT = process.env.MAILPIT_URL ?? "http://localhost:8025";

/**
 * The six-digit code of the last mail sent to `to`, waiting for it to arrive. Propolis writes it in
 * two groups of three (`869 239`); the field takes it without the space.
 */
async function confirmationCode(request: APIRequestContext, to: string): Promise<string> {
  let code: string | undefined;
  await expect
    .poll(
      async () => {
        const search = await request.get(`${MAILPIT}/api/v1/search`, { params: { query: `to:"${to}"` } });
        const found = (await search.json()) as { messages: { ID: string }[] };
        if (found.messages.length === 0) return false;
        const message = await request.get(`${MAILPIT}/api/v1/message/${found.messages[0].ID}`);
        const { Text } = (await message.json()) as { Text: string };
        code = Text.match(/\b(\d{3}) ?(\d{3})\b/)?.slice(1).join("");
        return code !== undefined;
      },
      { timeout: 30_000, message: `no confirmation code reached ${to}` },
    )
    .toBe(true);
  return code as string;
}

test("a new person signs up, confirms their address and signs in", async ({ page, request }) => {
  const email = `ada.${Date.now()}@example.org`;
  const password = "Analytical-Engine-1843-notes";

  await page.goto("/sign-up/");
  await page.locator("#first").fill("Ada");
  await page.locator("#last").fill("Lovelace");
  await page.locator("#s-mail").fill(email);
  await page.locator("#s-pw").fill(password);
  // The box itself is drawn by its label (the input is visually hidden): a person clicks the label.
  await page.locator("label", { has: page.getByRole("checkbox") }).click();
  await expect(page.getByRole("checkbox")).toBeChecked();
  await page.locator("form button[type=submit]").click();

  const code = await confirmationCode(request, email);
  await page.locator("#verify-code").fill(code);
  await page.locator("#verify-code").press("Enter");
  // Confirmed: the code field gives way to the invitation to sign in.
  await expect(page.locator("#verify-code")).toBeHidden();

  await page.goto("/sign-in/");
  await page.locator("#mail").fill(email);
  await page.locator("#pw").fill(password);
  await page.locator("form button[type=submit]").click();

  await expect(page).toHaveURL(/\/account\//);
  await expect(page.getByRole("heading", { level: 1 })).toHaveText("Ada Lovelace");
});
