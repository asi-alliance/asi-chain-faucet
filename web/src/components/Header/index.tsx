import UserManual from "@components/UserManual";
import AsiLogoSrc from "@assets/images/asi_logo.svg";
import { type ReactElement } from "react";
import "./style.css";

const Header = (): ReactElement => {
    return (
        <header>
            <div className="content-container">
                <div className="header-content">
                    <div className="header-logo">
                        <img src={AsiLogoSrc} alt="asi-logo" />
                    </div>
                    <h2>
                        ASI:Chain Faucet
                    </h2>
                    <UserManual />
                </div>
            </div>
        </header>
    );
};
     
export default Header;
