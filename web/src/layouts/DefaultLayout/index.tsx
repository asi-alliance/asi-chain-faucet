import Header from "@components/Header";
import { type ReactNode, type ReactElement } from "react";
import "./style.css";
import FeedbackForm from "@components/community/FeedbackForm";

export interface IDefaultLayoutProps {
    children: ReactElement | ReactNode;
}

const DefaultLayout = ({ children }: IDefaultLayoutProps): ReactElement => {
    return (
        <div className="default-layout">
            <Header />
            <main className="content-container">{children}</main>
            <FeedbackForm />
        </div>
    );
};

export default DefaultLayout;
