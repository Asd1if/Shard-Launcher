import { backend, type Accounts } from "@/lib/backend";
import { createStore } from "@/lib/store";

const store = createStore<Accounts>({ selected: null, accounts: [] });
const dialog = createStore(false);

export const useAccounts = store.use;
export const useAccountsDialog = dialog.use;

export function currentAccount(accounts: Accounts) {
  return accounts.accounts.find((a) => a.id === accounts.selected) ?? null;
}

export function hasAccount() {
  return currentAccount(store.get()) !== null;
}

export const openAccountsDialog = () => dialog.set(true);
export const setAccountsDialogOpen = (open: boolean) => dialog.set(open);

export async function addOfflineAccount(name: string) {
  store.set(await backend.addOfflineAccount(name));
}

/** Скин офлайн-аккаунта: `null` — снять */
export async function setOfflineSkin(id: string, skin: string | null) {
  store.set(await backend.setAccountSkin(id, skin));
}

export async function selectAccount(id: string) {
  store.set(await backend.selectAccount(id));
}

export async function removeAccount(id: string) {
  store.set(await backend.removeAccount(id));
}

/** Вход через Microsoft: показать код, затем ждать, пока пользователь введёт его на сайте. */
export const startMicrosoftLogin = () => backend.msaStart();

export async function finishMicrosoftLogin() {
  store.set(await backend.msaFinish());
}

export const cancelMicrosoftLogin = () => backend.msaCancel();

void backend.accounts().then(store.set);
