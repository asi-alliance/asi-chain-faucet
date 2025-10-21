import UserManual from "@components/UserManual";
import Logo from "@components/Logo";
import { type ReactElement } from "react";
import "./style.css";

const Header = (): ReactElement => {
    return (
        <header>
            <div className="content-container">
                <div className="header-content">
                    <Logo size="medium" showText={true} />
                    <h2>
                        Faucet
                    </h2>
                    <UserManual />
                </div>
            </div>
        </header>
    );
};
     
export default Header;
