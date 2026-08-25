package com.avernet.bcs.provider.sdk;

/** Application extension point exposed by the Provider SDK demo. */
public abstract class ProviderService {
    /** Return a short name identifying the concrete implementation. */
    public abstract String implementation();

    /** Handle one demo invocation. */
    public abstract String invoke(String message) throws Exception;
}
