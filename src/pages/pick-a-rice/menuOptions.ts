export const MENU_ITEMS = ['back', 'revert', 'submit', 'credits'] as const;
export type MenuItem = (typeof MENU_ITEMS)[number];
