import logoUrl from "../assets/logo.png";

/** The BudgetCut brand mark, used in the sidebar, login and dashboard. */
export default function Logo({ className = "logo" }: { className?: string }) {
  return <img className={className} src={logoUrl} alt="BudgetCut" />;
}
