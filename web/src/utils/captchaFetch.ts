import { createCaptchaFetchHandler } from "waf-captcha-frontend";

export const getCaptchaFetch = () => {
    const keys = {
        JSAPI_URL: import.meta.env.VITE_JSAPI_URL,
        CAPTCHA_TOKEN: import.meta.env.VITE_CAPTCHA_TOKEN
    }

    if (!keys.CAPTCHA_TOKEN || !keys.JSAPI_URL) {
        throw new Error("Tokens undefined");
    }

    const captchaFetch = createCaptchaFetchHandler({
        API_KEY: keys.CAPTCHA_TOKEN,
        JSAPI_URL: keys.JSAPI_URL,
        captchaContainerId: "captcha-modal-container",
    });

    return captchaFetch;
}