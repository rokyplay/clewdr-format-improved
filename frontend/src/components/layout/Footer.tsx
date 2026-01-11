import React from "react";
import { useTranslation } from "react-i18next";

const Footer: React.FC = () => {
  const { t } = useTranslation();
  const currentYear = new Date().getFullYear();

  return (
    <footer className="mt-12 text-center text-gray-500 text-sm">
      <p>{t("app.footer", { year: currentYear })}</p>
      <p className="mt-1 text-xs text-gray-600">
        Based on <a href="https://github.com/Xerxes-2/clewdr" target="_blank" rel="noopener noreferrer" className="hover:text-gray-400">ClewdR</a> by Xerxes-2
      </p>
    </footer>
  );
};

export default Footer;
