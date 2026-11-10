const isErrorStatus = (status: string) =>
    /_ERROR$/i.test(status) || /Error$/i.test(status);

export { isErrorStatus };
