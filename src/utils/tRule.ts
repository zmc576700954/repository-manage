import { TFunction } from 'i18next';

interface RuleTrans {
  title: string;
  message: string;
}

export function tRule(
  t: TFunction,
  ruleId: string,
  vars: Record<string, string> = {},
): RuleTrans {
  const titleKey = `rule.${ruleId}.title`;
  const messageKey = `rule.${ruleId}.message`;

  let title = t(titleKey, { defaultValue: ruleId });
  if (title === ruleId) {
    console.warn(`[tRule] missing i18n key: ${titleKey}`);
  }

  const message = t(messageKey, {
    defaultValue: '',
    ...vars,
  });
  if (message === '') {
    console.warn(`[tRule] missing i18n key: ${messageKey}`);
  }

  return { title, message };
}
