var target = p.path ? byPath(p.path) : document.activeElement;
if (p.path && !target) return STALE;
var facts = {
  frameOrigin: location.origin,
  targetInForm: false,
  formHasPassword: false,
  formHasPayment: false,
  isSubmitter: false,
  isFileInput: false,
  formAction: null,
  linkTarget: null
};
if (!target || target === document.body) return facts;
function absolute(href) {
  try { return new URL(href, document.baseURI).href; } catch (e) { return null; }
}
function tokens(el) {
  return (el.getAttribute('autocomplete') || '').toLowerCase().split(/\s+/);
}
var CARD_NAME = /card.?num|cc.?num|cvc|cvv|csc|security.?code|expir/i;
function isPassword(el) {
  if (el.tagName === 'INPUT' && inputType(el) === 'password') return true;
  return tokens(el).some(function (t) { return t === 'current-password' || t === 'new-password'; });
}
function isPayment(el) {
  if (tokens(el).some(function (t) { return t.indexOf('cc-') === 0; })) return true;
  return CARD_NAME.test((el.getAttribute('name') || '') + ' ' + (el.id || ''));
}
var tag = target.tagName;
var type = tag === 'INPUT' ? inputType(target) : '';
facts.isFileInput = type === 'file';
var form = target.form || (target.closest ? target.closest('form') : null);
if (form) {
  facts.targetInForm = true;
  var fields = Array.prototype.slice.call(form.elements || []);
  facts.formHasPassword = fields.some(isPassword);
  facts.formHasPayment = fields.some(isPayment);
  facts.isSubmitter = (tag === 'BUTTON' && (target.getAttribute('type') || 'submit').toLowerCase() === 'submit') ||
    (tag === 'INPUT' && (type === 'submit' || type === 'image'));
  var action = (facts.isSubmitter && target.getAttribute('formaction')) || form.getAttribute('action') || location.href;
  facts.formAction = absolute(action);
}
var link = target.closest ? target.closest('a[href],area[href]') : null;
if (link) facts.linkTarget = absolute(link.getAttribute('href'));
return facts;
